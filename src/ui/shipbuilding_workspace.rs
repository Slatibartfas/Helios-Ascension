use std::collections::HashMap;

use bevy::ecs::system::SystemParam;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::dashboard::format_mass_compact_tonnes;
use super::shipbuilding_state::{ShipbuildingTab, ShipbuildingUiState};
use super::shipbuilding_tooltip::{
    build_module_tooltip, build_slot_tooltip, format_shipbuilding_resource_cost_lines,
    prettify_slot_name, ShipbuildingTooltipContent, ShipbuildingTooltipEntry,
    ShipbuildingTooltipTone,
};
use super::theme;
use crate::colony::{BuildingType, Colony};
use crate::economy::components::LocalStockpile;
use crate::economy::GlobalBudget;
use crate::fleets::{ActiveManeuver, Fleet, FleetOrbit, ShipInstance};
use crate::game_state::{ActiveMenu, GameMenu};
use crate::plugins::solar_system::CelestialBody;
use crate::research::{
    EngineeringProject, PendingResearchActions, ResearchState, TechnologiesData,
};
use crate::shipbuilding::types::ShipModuleCategory;
use crate::shipbuilding::{
    HullSlotDefinition, LaunchCapacityState, PendingShipbuildingActions, QueueRefitAction,
    QueueShipConstructionAction, RefitProject, ShipConstructionProject, ShipDesignAssignment,
    ShipDesignDraft, ShipDesignLibrary, ShipDesignSummary, ShipModuleSelection, ShipbuildingData,
};

type WorkspaceColonyQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Colony,
        &'static CelestialBody,
        Option<&'static LocalStockpile>,
    ),
>;

type WorkspaceFleetQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Fleet,
        &'static FleetOrbit,
        Option<&'static ActiveManeuver>,
    ),
>;

type WorkspaceShipQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static ShipInstance, &'static ShipDesignAssignment)>;

#[derive(Clone)]
struct WorkspaceDesignRow {
    template_id: uuid::Uuid,
    name: String,
    version: u32,
    hull_name: String,
    hull_class: crate::fleets::ShipClass,
    summary: ShipDesignSummary,
    construction_mode: crate::shipbuilding::ConstructionMode,
    active_ship_count: usize,
}

#[derive(Clone)]
struct WorkspaceFleetRow {
    entity: Entity,
    name: String,
    orbit_radius_au: f64,
    stationary: bool,
    ship_count: usize,
}

#[derive(Clone, Copy)]
enum ShipbuildingArchiveAction {
    Open,
    Upgrade,
    QueueRetrofits,
    Delete,
}

pub(super) struct ShipbuildingWorkspacePlugin;

impl Plugin for ShipbuildingWorkspacePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_shipbuilding_workspace)
            .add_systems(Update, handle_shipbuilding_workspace_interactions)
            .add_systems(Update, handle_shipbuilding_tab_switch_buttons)
            .add_systems(Update, handle_shipbuilding_archive_interactions)
            .add_systems(Update, handle_shipbuilding_construction_interactions)
            .add_systems(Update, handle_shipbuilding_component_interactions)
            .add_systems(Update, animate_shipbuilding_slot_feedback)
            .add_systems(Update, animate_shipbuilding_module_card_feedback)
            .add_systems(Update, update_shipbuilding_hover_tooltip)
            .add_systems(Update, sync_shipbuilding_workspace_visibility)
            .add_systems(Update, sync_shipbuilding_workspace_content)
            .add_systems(
                Update,
                sync_library_filter_keyboard.before(sync_shipbuilding_workspace_content),
            );
    }
}

#[derive(Component)]
struct ShipbuildingWorkspaceRoot;

#[derive(Component)]
struct ShipbuildingWorkspaceStatus;

#[derive(Component)]
struct ShipbuildingWorkspaceTabs;

#[derive(Component)]
struct ShipbuildingWorkspaceLibrary;

#[derive(Component)]
struct ShipbuildingWorkspaceBlueprint;

#[derive(Component)]
struct ShipbuildingWorkspaceAnalytics;

#[derive(Component)]
struct ShipbuildingHoverTooltip;

#[derive(Component)]
struct ShipbuildingHoverTooltipTitle;

#[derive(Component)]
struct ShipbuildingHoverTooltipBody;

#[derive(Component)]
struct ShipbuildingSlotButton {
    slot_id: String,
}

#[derive(Component)]
struct ShipbuildingModuleButton {
    slot_id: String,
    module_id: String,
}

#[derive(Component)]
struct ShipbuildingHullDropdownToggle;

#[derive(Component)]
struct ShipbuildingHullOptionButton {
    hull_id: String,
}

#[derive(Component)]
struct ShipbuildingCategoryButton {
    category: ShipModuleCategory,
}

#[derive(Component)]
struct ShipbuildingClearSlotButton;

#[derive(Component)]
struct ShipbuildingLibraryFilterInput;

#[derive(Component)]
struct ShipbuildingClearLibraryFilterButton;

#[derive(Component)]
struct ShipbuildingWorkspaceTabButton {
    tab: ShipbuildingTab,
}

#[derive(Component)]
struct ShipbuildingTabSwitchButton {
    target: ShipbuildingTab,
}

#[derive(Component)]
struct ShipbuildingSaveDesignButton;

#[derive(Component)]
struct ShipbuildingResetDesignButton;

#[derive(Component)]
struct ShipbuildingArchiveSelectButton {
    template_id: uuid::Uuid,
}

#[derive(Component)]
struct ShipbuildingArchiveActionButton {
    template_id: uuid::Uuid,
    action: ShipbuildingArchiveAction,
}

#[derive(Component)]
struct ShipbuildingConstructionSiteButton {
    site: Entity,
}

#[derive(Component)]
struct ShipbuildingConstructionFleetButton {
    fleet: Option<Entity>,
}

#[derive(Component)]
struct ShipbuildingConstructionDesignButton {
    template_id: uuid::Uuid,
}

#[derive(Component)]
struct ShipbuildingQueueSelectedDesignButton;

#[derive(Component)]
struct ShipbuildingComponentDatabaseButton {
    module_id: String,
}

#[derive(Component)]
struct ShipbuildingOpenEngineeringButton;

#[derive(Component)]
struct ShipbuildingSlotFrame {
    slot_id: String,
    accent: Color,
    filled: bool,
    previewed: bool,
}

#[derive(Component)]
struct ShipbuildingSlotGlow {
    slot_id: String,
    base_width: f32,
    base_height: f32,
    accent: Color,
    filled: bool,
}

#[derive(Component)]
struct ShipbuildingSlotAccentRail {
    slot_id: String,
    accent: Color,
}

#[derive(Component)]
struct ShipbuildingSlotBorderRunner {
    slot_id: String,
    width: f32,
    height: f32,
    phase_offset: f32,
    accent: Color,
    trail_offset: f32,
    alpha_scale: f32,
    size: f32,
}

#[derive(Component)]
struct ShipbuildingModuleCard {
    slot_id: String,
    module_id: String,
    base_height: f32,
}

#[derive(SystemParam)]
struct ShipbuildingWorkspacePanels<'w, 's> {
    status_text: Single<
        'w,
        's,
        &'static mut Text,
        (
            With<ShipbuildingWorkspaceStatus>,
            Without<ShipbuildingWorkspaceAnalytics>,
            Without<ShipbuildingWorkspaceBlueprint>,
        ),
    >,
    tabs_root: Single<
        'w,
        's,
        Entity,
        (
            With<ShipbuildingWorkspaceTabs>,
            Without<ShipbuildingWorkspaceLibrary>,
            Without<ShipbuildingWorkspaceBlueprint>,
            Without<ShipbuildingWorkspaceAnalytics>,
        ),
    >,
    library_root: Single<
        'w,
        's,
        Entity,
        (
            With<ShipbuildingWorkspaceLibrary>,
            Without<ShipbuildingWorkspaceBlueprint>,
            Without<ShipbuildingWorkspaceAnalytics>,
        ),
    >,
    blueprint_root: Single<
        'w,
        's,
        Entity,
        (
            With<ShipbuildingWorkspaceBlueprint>,
            Without<ShipbuildingWorkspaceLibrary>,
            Without<ShipbuildingWorkspaceAnalytics>,
        ),
    >,
    analytics_root: Single<
        'w,
        's,
        Entity,
        (
            With<ShipbuildingWorkspaceAnalytics>,
            Without<ShipbuildingWorkspaceLibrary>,
            Without<ShipbuildingWorkspaceBlueprint>,
        ),
    >,
    child_lists: Query<'w, 's, &'static Children>,
}

fn spawn_shipbuilding_workspace(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Shipbuilding Native Workspace"),
            ShipbuildingWorkspaceRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(126.0),
                bottom: Val::Px(42.0),
                display: Display::None,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(12.0)),
                row_gap: Val::Px(8.0),
                min_height: Val::Px(0.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.02, 0.035, 0.96)),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        min_height: Val::Px(34.0),
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.03, 0.08, 0.12, 0.96)),
                    BorderColor::all(Color::srgb(0.22, 0.72, 0.86)),
                ))
                .with_children(|banner| {
                    banner.spawn((
                        ShipbuildingWorkspaceStatus,
                        Text::new("Initializing shipbuilding workspace..."),
                        TextFont {
                            font_size: 12.5,
                            ..default()
                        },
                        TextColor(Color::srgb(0.82, 0.94, 0.98)),
                    ));
                });

            parent.spawn((
                ShipbuildingWorkspaceTabs,
                Node {
                    width: Val::Percent(100.0),
                    min_height: Val::Px(36.0),
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    ..default()
                },
            ));

            parent
                .spawn((
                    Name::new("Shipbuilding Workspace Columns"),
                    Node {
                        width: Val::Percent(100.0),
                        flex_grow: 1.0,
                        flex_basis: Val::Px(0.0),
                        min_height: Val::Px(0.0),
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(10.0),
                        ..default()
                    },
                ))
                .with_children(|columns| {
                    spawn_panel(
                        columns,
                        "Logistics Hub",
                        Val::Px(250.0),
                        ShipbuildingWorkspaceLibrary,
                    );
                    spawn_panel(
                        columns,
                        "Design Blueprint",
                        Val::Auto,
                        ShipbuildingWorkspaceBlueprint,
                    );
                    spawn_panel(
                        columns,
                        "Engineering Analytics",
                        Val::Px(270.0),
                        ShipbuildingWorkspaceAnalytics,
                    );
                });

            parent
                .spawn((
                    ShipbuildingHoverTooltip,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Px(344.0),
                        padding: UiRect::all(Val::Px(8.0)),
                        display: Display::None,
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.02, 0.04, 0.07, 0.96)),
                    BorderColor::all(Color::srgb(0.22, 0.72, 0.86)),
                ))
                .with_children(|tooltip| {
                    tooltip.spawn((
                        ShipbuildingHoverTooltipTitle,
                        Text::new(""),
                        TextFont {
                            font_size: 11.5,
                            ..default()
                        },
                        TextColor(Color::srgb(0.55, 0.95, 1.0)),
                    ));
                    tooltip.spawn((
                        ShipbuildingHoverTooltipBody,
                        Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(3.0),
                            ..default()
                        },
                    ));
                });
        });
}

fn spawn_panel<T: Component>(
    parent: &mut ChildSpawnerCommands,
    title: &str,
    width: Val,
    marker: T,
) {
    let mut node = Node {
        min_height: Val::Px(0.0),
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(Val::Px(10.0)),
        row_gap: Val::Px(8.0),
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    };

    if width == Val::Auto {
        node.flex_grow = 1.0;
        node.flex_basis = Val::Px(0.0);
    } else {
        node.width = width;
    }

    parent
        .spawn((
            Name::new(title.to_string()),
            node,
            BackgroundColor(Color::srgba(0.03, 0.05, 0.08, 0.92)),
            BorderColor::all(Color::srgb(0.15, 0.78, 0.88)),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new(title),
                TextFont {
                    font_size: 15.0,
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.95, 1.0)),
            ));
            panel.spawn((
                marker,
                Node {
                    width: Val::Percent(100.0),
                    flex_grow: 1.0,
                    flex_basis: Val::Px(0.0),
                    min_height: Val::Px(0.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    ..default()
                },
            ));
        });
}

fn sync_shipbuilding_workspace_visibility(
    active_menu: Res<ActiveMenu>,
    mut roots: Query<&mut Node, With<ShipbuildingWorkspaceRoot>>,
) {
    if !active_menu.is_changed() {
        return;
    }

    let display = if active_menu.current == GameMenu::Shipbuilding {
        Display::Flex
    } else {
        Display::None
    };

    for mut node in &mut roots {
        node.display = display;
    }
}

fn handle_shipbuilding_workspace_interactions(
    active_menu: Res<ActiveMenu>,
    shipbuilding_data: Res<ShipbuildingData>,
    research_state: Res<ResearchState>,
    mut design_library: ResMut<ShipDesignLibrary>,
    mut ui_state: ResMut<ShipbuildingUiState>,
    tab_buttons: Query<
        (&Interaction, &ShipbuildingWorkspaceTabButton),
        (Changed<Interaction>, With<Button>),
    >,
    hull_dropdown_toggle: Query<
        &Interaction,
        (
            Changed<Interaction>,
            With<ShipbuildingHullDropdownToggle>,
            With<Button>,
        ),
    >,
    hull_option_buttons: Query<
        (&Interaction, &ShipbuildingHullOptionButton),
        (Changed<Interaction>, With<Button>),
    >,
    category_buttons: Query<
        (&Interaction, &ShipbuildingCategoryButton),
        (Changed<Interaction>, With<Button>),
    >,
    clear_buttons: Query<
        (
            &Interaction,
            Has<ShipbuildingClearSlotButton>,
            Has<ShipbuildingClearLibraryFilterButton>,
        ),
        (
            Changed<Interaction>,
            Or<(
                With<ShipbuildingClearSlotButton>,
                With<ShipbuildingClearLibraryFilterButton>,
            )>,
            With<Button>,
        ),
    >,
    clear_filter_buttons: Query<
        &Interaction,
        (
            Changed<Interaction>,
            With<ShipbuildingClearLibraryFilterButton>,
            With<Button>,
        ),
    >,
    slot_press_buttons: Query<
        (&Interaction, &ShipbuildingSlotButton),
        (Changed<Interaction>, With<Button>),
    >,
    slot_hover_buttons: Query<(&Interaction, &ShipbuildingSlotButton), With<Button>>,
    module_press_buttons: Query<
        (&Interaction, &ShipbuildingModuleButton),
        (Changed<Interaction>, With<Button>),
    >,
    module_hover_buttons: Query<(&Interaction, &ShipbuildingModuleButton), With<Button>>,
    save_buttons: Query<
        &Interaction,
        (
            Changed<Interaction>,
            With<ShipbuildingSaveDesignButton>,
            With<Button>,
        ),
    >,
    reset_buttons: Query<
        &Interaction,
        (
            Changed<Interaction>,
            With<ShipbuildingResetDesignButton>,
            With<Button>,
        ),
    >,
) {
    if active_menu.current != GameMenu::Shipbuilding {
        return;
    }

    let mut content_changed = false;
    {
        let ui_state = &mut *ui_state;

        if ui_state.selected_hull_id.is_none() {
            if let Some(hull) = shipbuilding_data.available_hulls(&research_state).first() {
                select_hull_by_id(ui_state, &shipbuilding_data, &research_state, &hull.id);
                content_changed = true;
            }
        }

        if ui_state
            .selected_hull_id
            .as_deref()
            .and_then(|hull_id| shipbuilding_data.get_hull(hull_id))
            .is_some_and(|hull| !shipbuilding_data.hull_is_unlocked(hull, &research_state))
        {
            if let Some(hull) = shipbuilding_data.available_hulls(&research_state).first() {
                select_hull_by_id(ui_state, &shipbuilding_data, &research_state, &hull.id);
            } else {
                ui_state.selected_hull_id = None;
                ui_state.selected_template_id = None;
                ui_state.upgrade_source_template_id = None;
                ui_state.design_name.clear();
                ui_state.selected_modules.clear();
                ui_state.selected_slot = None;
                ui_state.preview_slot = None;
                ui_state.preview_module_id = None;
                ui_state.show_hull_dropdown = false;
            }
            content_changed = true;
        }

        if ui_state.selected_slot.is_none() {
            if let Some(hull_id) = ui_state.selected_hull_id.as_deref() {
                if let Some(hull) = shipbuilding_data.get_hull(hull_id) {
                    if let Some(slot) = hull.slot_layout.first() {
                        ui_state.selected_slot = Some(slot.slot_id.clone());
                        content_changed = true;
                    }
                }
            }
        }

        for (interaction, button) in &tab_buttons {
            if *interaction != Interaction::Pressed {
                continue;
            }

            ui_state.active_tab = button.tab;
            content_changed = true;
        }

        for interaction in &save_buttons {
            if *interaction == Interaction::Pressed
                && save_current_design_template_native(&mut design_library, ui_state).is_some()
            {
                content_changed = true;
            }
        }

        for interaction in &reset_buttons {
            if *interaction == Interaction::Pressed {
                if let Some(hull_id) = ui_state.selected_hull_id.clone() {
                    select_hull_by_id(ui_state, &shipbuilding_data, &research_state, &hull_id);
                    content_changed = true;
                }
            }
        }

        for interaction in &hull_dropdown_toggle {
            if *interaction == Interaction::Pressed {
                ui_state.show_hull_dropdown = !ui_state.show_hull_dropdown;
                content_changed = true;
            }
        }

        for (interaction, button) in &hull_option_buttons {
            if *interaction == Interaction::Pressed {
                select_hull_by_id(
                    ui_state,
                    &shipbuilding_data,
                    &research_state,
                    button.hull_id.as_str(),
                );
                ui_state.show_hull_dropdown = false;
                content_changed = true;
            }
        }

        for (interaction, button) in &category_buttons {
            if *interaction == Interaction::Pressed {
                select_first_slot_in_category(ui_state, &shipbuilding_data, button.category);
                content_changed = true;
            }
        }

        for (interaction, is_clear_slot, is_clear_filter) in &clear_buttons {
            if *interaction != Interaction::Pressed {
                continue;
            }
            if is_clear_slot {
                if let Some(slot_id) = ui_state.selected_slot.clone() {
                    ui_state.selected_modules.remove(&slot_id);
                    ui_state.preview_slot = Some(slot_id);
                    ui_state.preview_module_id = None;
                    content_changed = true;
                }
            } else if is_clear_filter && !ui_state.library_filter_query.is_empty() {
                ui_state.library_filter_query.clear();
                content_changed = true;
            }
        }

        for interaction in &clear_filter_buttons {
            if *interaction == Interaction::Pressed && !ui_state.library_filter_query.is_empty() {
                ui_state.library_filter_query.clear();
                content_changed = true;
            }
        }

        for (interaction, slot_button) in &slot_press_buttons {
            if *interaction == Interaction::Pressed {
                ui_state.selected_slot = Some(slot_button.slot_id.clone());
                ui_state.preview_slot = None;
                ui_state.preview_module_id = None;
                content_changed = true;
            }
        }

        let mut hovered_preview = None;
        let mut clear_preview = false;
        let mut hovered_slot = None;
        let mut hovered_module = None;

        for (interaction, module_button) in &module_press_buttons {
            match *interaction {
                Interaction::Pressed => {
                    ui_state.selected_slot = Some(module_button.slot_id.clone());
                    ui_state.selected_modules.insert(
                        module_button.slot_id.clone(),
                        module_button.module_id.clone(),
                    );
                    ui_state.preview_slot = Some(module_button.slot_id.clone());
                    ui_state.preview_module_id = Some(module_button.module_id.clone());
                    content_changed = true;
                }
                Interaction::Hovered => {}
                Interaction::None => {
                    if ui_state.preview_slot.as_deref() == Some(module_button.slot_id.as_str())
                        && ui_state.preview_module_id.as_deref()
                            == Some(module_button.module_id.as_str())
                    {
                        clear_preview = true;
                    }
                }
            }
        }

        for (interaction, slot_button) in &slot_hover_buttons {
            if *interaction == Interaction::Hovered {
                hovered_slot = Some(slot_button.slot_id.clone());
            }
        }

        for (interaction, module_button) in &module_hover_buttons {
            if *interaction == Interaction::Hovered {
                hovered_preview = Some((
                    module_button.slot_id.clone(),
                    module_button.module_id.clone(),
                ));
                hovered_module = Some(module_button.module_id.clone());
                hovered_slot = Some(module_button.slot_id.clone());
            }
        }

        if let Some((slot_id, module_id)) = hovered_preview {
            if ui_state.preview_slot.as_deref() != Some(slot_id.as_str())
                || ui_state.preview_module_id.as_deref() != Some(module_id.as_str())
            {
                ui_state.preview_slot = Some(slot_id);
                ui_state.preview_module_id = Some(module_id);
                content_changed = true;
            }
        } else if clear_preview
            && (ui_state.preview_slot.is_some() || ui_state.preview_module_id.is_some())
        {
            ui_state.preview_slot = None;
            ui_state.preview_module_id = None;
            content_changed = true;
        }

        if ui_state.hovered_slot != hovered_slot || ui_state.hovered_module_id != hovered_module {
            ui_state.hovered_slot = hovered_slot;
            ui_state.hovered_module_id = hovered_module;
        }
    }

    let _ = content_changed;
}

fn handle_shipbuilding_tab_switch_buttons(
    active_menu: Res<ActiveMenu>,
    mut ui_state: ResMut<ShipbuildingUiState>,
    tab_switch_buttons: Query<
        (&Interaction, &ShipbuildingTabSwitchButton),
        (Changed<Interaction>, With<Button>),
    >,
) {
    if active_menu.current != GameMenu::Shipbuilding {
        return;
    }

    for (interaction, button) in &tab_switch_buttons {
        if *interaction == Interaction::Pressed {
            ui_state.active_tab = button.target;
        }
    }
}

fn handle_shipbuilding_archive_interactions(
    active_menu: Res<ActiveMenu>,
    shipbuilding_data: Res<ShipbuildingData>,
    research_state: Res<ResearchState>,
    mut design_library: ResMut<ShipDesignLibrary>,
    mut shipbuilding_actions: ResMut<PendingShipbuildingActions>,
    mut ui_state: ResMut<ShipbuildingUiState>,
    archive_select_buttons: Query<
        (&Interaction, &ShipbuildingArchiveSelectButton),
        (Changed<Interaction>, With<Button>),
    >,
    archive_action_buttons: Query<
        (&Interaction, &ShipbuildingArchiveActionButton),
        (Changed<Interaction>, With<Button>),
    >,
    site_buttons: Query<
        (&Interaction, &ShipbuildingConstructionSiteButton),
        (Changed<Interaction>, With<Button>),
    >,
    ships: WorkspaceShipQuery,
    projects: Query<&ShipConstructionProject>,
    refits: Query<&RefitProject>,
) {
    if active_menu.current != GameMenu::Shipbuilding
        || ui_state.active_tab != ShipbuildingTab::Archive
    {
        return;
    }

    let mut content_changed = false;
    let ui_state = &mut *ui_state;

    for (interaction, button) in &archive_select_buttons {
        if *interaction == Interaction::Pressed {
            ui_state.selected_template_id = Some(button.template_id);
            content_changed = true;
        }
    }

    for (interaction, button) in &site_buttons {
        if *interaction == Interaction::Pressed {
            ui_state.selected_colony = Some(button.site);
            content_changed = true;
        }
    }

    for (interaction, button) in &archive_action_buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match button.action {
            ShipbuildingArchiveAction::Open => {
                if let Some(template) = design_library.get_template(&button.template_id) {
                    ui_state.upgrade_source_template_id = None;
                    load_template_into_ui_native(
                        ui_state,
                        &shipbuilding_data,
                        &research_state,
                        template,
                    );
                    ui_state.active_tab = ShipbuildingTab::Design;
                }
            }
            ShipbuildingArchiveAction::Upgrade => {
                if let Some(template) = design_library.get_template(&button.template_id) {
                    load_template_into_ui_native(
                        ui_state,
                        &shipbuilding_data,
                        &research_state,
                        template,
                    );
                    ui_state.upgrade_source_template_id = Some(template.id);
                    ui_state.active_tab = ShipbuildingTab::Design;
                }
            }
            ShipbuildingArchiveAction::QueueRetrofits => {
                let Some(build_site) = ui_state.selected_colony else {
                    continue;
                };
                let active_refits: std::collections::HashSet<_> =
                    refits.iter().map(|refit| refit.ship_entity).collect();

                for (ship_entity, ship, assignment) in &ships {
                    if ship.parked_body != build_site
                        || assignment.template_id == button.template_id
                        || active_refits.contains(&ship_entity)
                    {
                        continue;
                    }

                    if template_descends_from_native(
                        &design_library,
                        button.template_id,
                        assignment.template_id,
                    ) {
                        shipbuilding_actions.queue_refits.push(QueueRefitAction {
                            ship_entity,
                            new_template_id: button.template_id,
                            build_site,
                        });
                    }
                }
            }
            ShipbuildingArchiveAction::Delete => {
                let is_referenced = ships
                    .iter()
                    .any(|(_, _, assignment)| assignment.template_id == button.template_id)
                    || projects
                        .iter()
                        .any(|project| project.template_id == button.template_id)
                    || refits.iter().any(|refit| {
                        refit.old_template_id == button.template_id
                            || refit.new_template_id == button.template_id
                    })
                    || template_has_descendants_native(&design_library, button.template_id);

                if !is_referenced {
                    design_library.templates.remove(&button.template_id);
                    if ui_state.selected_template_id == Some(button.template_id) {
                        ui_state.selected_template_id = None;
                    }
                    if ui_state.construction_design_id == Some(button.template_id) {
                        ui_state.construction_design_id = None;
                    }
                }
            }
        }

        content_changed = true;
    }

    let _ = content_changed;
}

fn handle_shipbuilding_construction_interactions(
    active_menu: Res<ActiveMenu>,
    shipbuilding_data: Res<ShipbuildingData>,
    research_state: Res<ResearchState>,
    design_library: Res<ShipDesignLibrary>,
    mut shipbuilding_actions: ResMut<PendingShipbuildingActions>,
    colonies: WorkspaceColonyQuery,
    mut ui_state: ResMut<ShipbuildingUiState>,
    site_buttons: Query<
        (&Interaction, &ShipbuildingConstructionSiteButton),
        (Changed<Interaction>, With<Button>),
    >,
    fleet_buttons: Query<
        (&Interaction, &ShipbuildingConstructionFleetButton),
        (Changed<Interaction>, With<Button>),
    >,
    construction_design_buttons: Query<
        (&Interaction, &ShipbuildingConstructionDesignButton),
        (Changed<Interaction>, With<Button>),
    >,
    queue_buttons: Query<
        &Interaction,
        (
            Changed<Interaction>,
            With<ShipbuildingQueueSelectedDesignButton>,
            With<Button>,
        ),
    >,
) {
    if active_menu.current != GameMenu::Shipbuilding
        || ui_state.active_tab != ShipbuildingTab::Construction
    {
        return;
    }

    let mut content_changed = false;
    let ui_state = &mut *ui_state;

    for (interaction, button) in &site_buttons {
        if *interaction == Interaction::Pressed {
            ui_state.selected_colony = Some(button.site);
            ui_state.construction_target_fleet = None;
            content_changed = true;
        }
    }

    for (interaction, button) in &fleet_buttons {
        if *interaction == Interaction::Pressed {
            ui_state.construction_target_fleet = button.fleet;
            content_changed = true;
        }
    }

    for (interaction, button) in &construction_design_buttons {
        if *interaction == Interaction::Pressed {
            ui_state.construction_design_id = Some(button.template_id);
            content_changed = true;
        }
    }

    for interaction in &queue_buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }

        let selected_colony = ui_state
            .selected_colony
            .and_then(|entity| colonies.get(entity).ok());
        let Some(template_id) = ui_state.construction_design_id else {
            continue;
        };
        let Some(template) = design_library.get_template(&template_id) else {
            continue;
        };

        let draft = design_from_template_native(template);
        let summary = shipbuilding_data.summarize_design(&draft, &research_state);
        let hull = shipbuilding_data.get_hull(&template.hull_id);
        let queue_errors = crate::shipbuilding::systems::queue_validation_errors(
            selected_colony.map(|(_, colony, _, _)| colony),
            hull,
            summary.as_ref(),
            template.construction_mode,
        );

        if queue_errors.is_empty() {
            if let Some((build_site, _, _, _)) = selected_colony {
                shipbuilding_actions
                    .queue_projects
                    .push(QueueShipConstructionAction {
                        build_site,
                        template_id,
                        integration_target_fleet: ui_state.construction_target_fleet,
                    });
                content_changed = true;
            }
        }
    }

    let _ = content_changed;
}

fn handle_shipbuilding_component_interactions(
    active_menu: Res<ActiveMenu>,
    shipbuilding_data: Res<ShipbuildingData>,
    mut pending_research: ResMut<PendingResearchActions>,
    mut ui_state: ResMut<ShipbuildingUiState>,
    component_buttons: Query<
        (&Interaction, &ShipbuildingComponentDatabaseButton),
        (Changed<Interaction>, With<Button>),
    >,
    open_engineering_buttons: Query<
        &Interaction,
        (
            Changed<Interaction>,
            With<ShipbuildingOpenEngineeringButton>,
            With<Button>,
        ),
    >,
) {
    if active_menu.current != GameMenu::Shipbuilding
        || ui_state.active_tab != ShipbuildingTab::Components
    {
        return;
    }

    let mut content_changed = false;
    let ui_state = &mut *ui_state;

    for (interaction, button) in &component_buttons {
        if *interaction == Interaction::Pressed {
            ui_state.selected_component_module_id = Some(button.module_id.clone());
            content_changed = true;
        }
    }

    for interaction in &open_engineering_buttons {
        if *interaction == Interaction::Pressed {
            pending_research.navigate_to_available_engineering_tab = true;
            pending_research.navigate_to_engineering_target = ui_state
                .selected_component_module_id
                .as_deref()
                .and_then(|module_id| shipbuilding_data.get_module(module_id))
                .map(|module| module.engineering_project_id().to_string());
        }
    }

    let _ = content_changed;
}

fn sync_shipbuilding_workspace_content(
    mut commands: Commands,
    active_menu: Res<ActiveMenu>,
    ui_state: Res<ShipbuildingUiState>,
    shipbuilding_data: Res<ShipbuildingData>,
    design_library: Res<ShipDesignLibrary>,
    research_state: Res<ResearchState>,
    technologies_data: Res<TechnologiesData>,
    colonies: WorkspaceColonyQuery,
    fleets: WorkspaceFleetQuery,
    ships: WorkspaceShipQuery,
    projects: Query<(Entity, &ShipConstructionProject)>,
    refits: Query<(Entity, &RefitProject)>,
    engineering_projects: Query<&EngineeringProject>,
    launch_state: Res<LaunchCapacityState>,
    budget: Res<GlobalBudget>,
    mut panels: ShipbuildingWorkspacePanels,
) {
    if active_menu.current != GameMenu::Shipbuilding {
        return;
    }

    if !active_menu.is_changed()
        && !ui_state.is_changed()
        && !shipbuilding_data.is_changed()
        && !research_state.is_changed()
        && !technologies_data.is_changed()
        && !design_library.is_changed()
        && !matches!(
            ui_state.active_tab,
            ShipbuildingTab::Archive | ShipbuildingTab::Construction
        )
    {
        return;
    }

    let available_hulls = shipbuilding_data.available_hulls(&research_state);
    let selected_hull = ui_state
        .selected_hull_id
        .as_deref()
        .and_then(|hull_id| shipbuilding_data.get_hull(hull_id))
        .filter(|hull| shipbuilding_data.hull_is_unlocked(hull, &research_state));
    let current_design = build_preview_design(&ui_state);
    let current_summary = current_design
        .as_ref()
        .and_then(|design| shipbuilding_data.summarize_design(design, &research_state));
    let preview_summary = build_preview_summary(&ui_state, &shipbuilding_data, &research_state);
    let active_slot = selected_hull.and_then(|hull| active_slot(hull, &ui_state));

    **panels.status_text = Text::new(format!(
        "Shipbuilding Workspace | {:?} | Hulls {} | Designs {} | Native-only workflow",
        ui_state.active_tab,
        available_hulls.len(),
        design_library.templates.len()
    ));

    clear_dynamic_children(&mut commands, *panels.tabs_root, &panels.child_lists);
    clear_dynamic_children(&mut commands, *panels.library_root, &panels.child_lists);
    clear_dynamic_children(&mut commands, *panels.blueprint_root, &panels.child_lists);
    clear_dynamic_children(&mut commands, *panels.analytics_root, &panels.child_lists);

    populate_tab_strip(&mut commands, *panels.tabs_root, ui_state.active_tab);

    match ui_state.active_tab {
        ShipbuildingTab::Design => {
            populate_library_panel(
                &mut commands,
                *panels.library_root,
                &available_hulls,
                selected_hull,
                active_slot,
                &ui_state,
                &shipbuilding_data,
                &research_state,
            );
            populate_blueprint_panel(
                &mut commands,
                *panels.blueprint_root,
                selected_hull,
                &ui_state,
                &shipbuilding_data,
            );
            populate_analytics_panel(
                &mut commands,
                *panels.analytics_root,
                selected_hull,
                current_summary.as_ref(),
                preview_summary.as_ref(),
                &ui_state,
            );
        }
        ShipbuildingTab::Archive => populate_archive_tab_native(
            &mut commands,
            *panels.library_root,
            *panels.blueprint_root,
            *panels.analytics_root,
            &colonies,
            &ships,
            &refits,
            &design_library,
            &shipbuilding_data,
            &research_state,
            &ui_state,
        ),
        ShipbuildingTab::Construction => populate_construction_tab_native(
            &mut commands,
            *panels.library_root,
            *panels.blueprint_root,
            *panels.analytics_root,
            &colonies,
            &fleets,
            &ships,
            &projects,
            &design_library,
            &shipbuilding_data,
            &research_state,
            &launch_state,
            &budget,
            &ui_state,
        ),
        ShipbuildingTab::Components => populate_components_tab_native(
            &mut commands,
            *panels.library_root,
            *panels.blueprint_root,
            *panels.analytics_root,
            &shipbuilding_data,
            &technologies_data,
            &research_state,
            &engineering_projects,
            &ui_state,
        ),
    }
}

fn populate_tab_strip(commands: &mut Commands, tabs_root: Entity, active_tab: ShipbuildingTab) {
    commands.entity(tabs_root).with_children(|parent| {
        for (tab, label) in [
            (ShipbuildingTab::Design, "Design"),
            (ShipbuildingTab::Archive, "Archive"),
            (ShipbuildingTab::Construction, "Construction Control"),
            (ShipbuildingTab::Components, "Component Database"),
        ] {
            let selected = tab == active_tab;
            parent.spawn((
                Button,
                ShipbuildingWorkspaceTabButton { tab },
                Node {
                    min_width: Val::Px(if tab == ShipbuildingTab::Components {
                        188.0
                    } else if tab == ShipbuildingTab::Construction {
                        168.0
                    } else {
                        136.0
                    }),
                    min_height: Val::Px(30.0),
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(if selected {
                    Color::srgb(0.1, 0.28, 0.34)
                } else {
                    Color::srgb(0.045, 0.07, 0.1)
                }),
                BorderColor::all(if selected {
                    Color::srgb(0.0, 0.95, 1.0)
                } else {
                    Color::srgb(0.18, 0.3, 0.36)
                }),
                Text::new(label),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(if selected {
                    Color::srgb(0.9, 0.98, 1.0)
                } else {
                    Color::srgb(0.78, 0.86, 0.9)
                }),
            ));
        }
    });
}

fn build_preview_design(ui_state: &ShipbuildingUiState) -> Option<ShipDesignDraft> {
    let hull_id = ui_state.selected_hull_id.clone()?;

    Some(ShipDesignDraft {
        name: if ui_state.design_name.trim().is_empty() {
            "Untitled Design".to_string()
        } else {
            ui_state.design_name.clone()
        },
        hull_id,
        modules: ui_state
            .selected_modules
            .iter()
            .map(|(slot_id, module_id)| ShipModuleSelection {
                slot_id: slot_id.clone(),
                module_id: module_id.clone(),
            })
            .collect(),
        construction_mode: ui_state.selected_mode,
    })
}

fn clear_dynamic_children(commands: &mut Commands, entity: Entity, child_lists: &Query<&Children>) {
    if let Ok(children) = child_lists.get(entity) {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
}

fn populate_library_panel(
    commands: &mut Commands,
    library_root: Entity,
    available_hulls: &[&crate::shipbuilding::ShipHullDefinition],
    selected_hull: Option<&crate::shipbuilding::ShipHullDefinition>,
    active_slot: Option<&HullSlotDefinition>,
    ui_state: &ShipbuildingUiState,
    shipbuilding_data: &ShipbuildingData,
    research_state: &ResearchState,
) {
    commands.entity(library_root).with_children(|parent| {
        spawn_hull_controls(parent, available_hulls, selected_hull, ui_state);
        spawn_category_controls(parent, selected_hull, ui_state);

        parent.spawn(text_block(
            match selected_hull {
                Some(hull) => format!(
                    "Design: {}\nHull: {}\nFocused slot: {}",
                    effective_design_name(ui_state, hull.display_name.as_str()),
                    hull.display_name,
                    active_slot
                        .map(|slot| prettify_slot_name(&slot.slot_id))
                        .unwrap_or_else(|| "Select a slot".to_string())
                ),
                None => "No hull selected. Use the hull controls above to seed a design directly in the native workspace.".to_string(),
            },
            12.0,
            Color::srgb(0.82, 0.87, 0.9),
        ));

        parent
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    ..default()
                },
            ))
            .with_children(|row| {
                row.spawn((
                    Button,
                    ShipbuildingSaveDesignButton,
                    Node {
                        flex_grow: 1.0,
                        min_height: Val::Px(30.0),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.08, 0.18, 0.14)),
                    BorderColor::all(Color::srgb(0.38, 0.94, 0.7)),
                    Text::new("Save Design"),
                    TextFont {
                        font_size: 10.5,
                        ..default()
                    },
                    TextColor(Color::srgb(0.92, 0.96, 0.98)),
                ));
                row.spawn((
                    Button,
                    ShipbuildingResetDesignButton,
                    Node {
                        flex_grow: 1.0,
                        min_height: Val::Px(30.0),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.12, 0.08, 0.09)),
                    BorderColor::all(Color::srgb(0.86, 0.42, 0.38)),
                    Text::new("Reset Hull"),
                    TextFont {
                        font_size: 10.5,
                        ..default()
                    },
                    TextColor(Color::srgb(0.92, 0.96, 0.98)),
                ));
            });

        if let Some(slot) = active_slot {
            parent.spawn(text_block(
                format!(
                    "{} | {} slot",
                    slot.category.display_name(),
                    slot.size
                ),
                13.0,
                theme::module_category_color(slot.category),
            ));

            parent.spawn((
                Button,
                ShipbuildingClearSlotButton,
                Node {
                    width: Val::Percent(100.0),
                    min_height: Val::Px(28.0),
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.13, 0.08, 0.09)),
                BorderColor::all(Color::srgb(0.58, 0.3, 0.32)),
                Text::new("Clear Selected Slot"),
                TextFont {
                    font_size: 10.5,
                    ..default()
                },
                TextColor(Color::srgb(0.94, 0.84, 0.84)),
            ));

            spawn_library_filter_row(parent, &ui_state.library_filter_query);

            let all_compatible = shipbuilding_data.compatible_modules_for_slot(slot, research_state);
            let query_trimmed = ui_state.library_filter_query.trim();
            let filtered_modules: Vec<&crate::shipbuilding::ShipModuleDefinition> =
                if query_trimmed.is_empty() {
                    all_compatible.to_vec()
                } else {
                    let q = query_trimmed.to_lowercase();
                    all_compatible
                        .iter()
                        .copied()
                        .filter(|m| {
                            m.display_name.to_lowercase().contains(&q)
                                || m.id.to_lowercase().contains(&q)
                                || m.tags.iter().any(|t| t.to_lowercase().contains(&q))
                        })
                        .collect()
                };

            if !all_compatible.is_empty() && filtered_modules.is_empty() {
                parent.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        min_height: Val::Px(28.0),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.12, 0.1, 0.07, 0.92)),
                    BorderColor::all(Color::srgb(0.62, 0.42, 0.28)),
                ))
                .with_children(|empty| {
                    empty.spawn(text_block(
                        format!("No modules match '{}'.", query_trimmed),
                        11.0,
                        Color::srgb(0.92, 0.78, 0.62),
                    ));
                    empty.spawn((
                        Button,
                        ShipbuildingClearLibraryFilterButton,
                        Node {
                            min_height: Val::Px(22.0),
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.18, 0.12, 0.08)),
                        BorderColor::all(Color::srgb(0.78, 0.52, 0.36)),
                        Text::new("Clear"),
                        TextFont {
                            font_size: 10.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.96, 0.86, 0.74)),
                    ));
                });
            } else {
                for module in filtered_modules {
                    let installed =
                        ui_state.selected_modules.get(&slot.slot_id) == Some(&module.id);
                    let previewed = ui_state.preview_slot.as_deref()
                        == Some(slot.slot_id.as_str())
                        && ui_state.preview_module_id.as_deref() == Some(module.id.as_str());
                    let color = if installed {
                        Color::srgb(0.1, 0.32, 0.22)
                    } else if previewed {
                        Color::srgb(0.12, 0.22, 0.34)
                    } else {
                        Color::srgb(0.055, 0.08, 0.12)
                    };

                    parent.spawn((
                        Button,
                        ShipbuildingModuleCard {
                            slot_id: slot.slot_id.clone(),
                            module_id: module.id.clone(),
                            base_height: 52.0,
                        },
                        ShipbuildingModuleButton {
                            slot_id: slot.slot_id.clone(),
                            module_id: module.id.clone(),
                        },
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(52.0),
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(color),
                        BorderColor::all(if installed {
                            Color::srgb(0.38, 0.94, 0.7)
                        } else if previewed {
                            Color::srgb(0.55, 0.95, 1.0)
                        } else {
                            Color::srgb(0.22, 0.35, 0.42)
                        }),
                        Text::new(format!(
                            "{}\n{}  {:.0} t  {:.0} BP\nNet {:+.0} MW  {:.0} kN",
                            module.display_name,
                            module.size,
                            module.dry_mass_t,
                            module.build_points,
                            module.power_generation_mw - module.power_draw_mw,
                            module.thrust_kn,
                        )),
                        TextFont {
                            font_size: 10.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.88, 0.93, 0.96)),
                    ));
                }
            }
        } else {
            parent.spawn(text_block(
                "Select a slot in the blueprint to narrow the module library. Compatible cards will appear here and can be clicked to install modules into the draft.".to_string(),
                11.0,
                Color::srgb(0.6, 0.7, 0.76),
            ));
        }
    });
}

fn spawn_library_filter_row(parent: &mut ChildSpawnerCommands, query: &str) {
    let trimmed = query.trim();
    let display_text = if trimmed.is_empty() {
        "type to filter compatible modules…".to_string()
    } else {
        query.to_string()
    };
    let display_color = if trimmed.is_empty() {
        Color::srgb(0.5, 0.6, 0.66)
    } else {
        Color::srgb(0.95, 0.98, 1.0)
    };

    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                min_height: Val::Px(24.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.07, 0.11, 0.92)),
            BorderColor::all(Color::srgb(0.22, 0.72, 0.86)),
            ShipbuildingLibraryFilterInput,
        ))
        .with_children(|row| {
            row.spawn(text_block(
                "Filter:".to_string(),
                11.0,
                Color::srgb(0.55, 0.95, 1.0),
            ));
            row.spawn(text_block(display_text, 11.0, display_color));
        });
}

fn sync_library_filter_keyboard(
    active_menu: Res<ActiveMenu>,
    mut ui_state: ResMut<ShipbuildingUiState>,
    mut keyboard_events: MessageReader<KeyboardInput>,
) {
    if active_menu.current != GameMenu::Shipbuilding {
        return;
    }
    if ui_state.selected_slot.is_none() {
        return;
    }

    let mut changed = false;
    for event in keyboard_events.read() {
        if !event.state.is_pressed() {
            continue;
        }
        match &event.logical_key {
            bevy::input::keyboard::Key::Backspace => {
                if !ui_state.library_filter_query.is_empty() {
                    ui_state.library_filter_query.pop();
                    changed = true;
                }
            }
            bevy::input::keyboard::Key::Escape => {
                if !ui_state.library_filter_query.is_empty() {
                    ui_state.library_filter_query.clear();
                    changed = true;
                }
            }
            _ => {
                if let Some(inserted) = event.text.as_deref() {
                    if inserted.chars().all(is_library_filter_printable) {
                        ui_state.library_filter_query.push_str(inserted);
                        changed = true;
                    }
                }
            }
        }
    }

    if changed {
        ui_state.set_changed();
    }
}

fn is_library_filter_printable(c: char) -> bool {
    if c.is_ascii_control() {
        return false;
    }
    !matches!(c, '\u{e000}'..='\u{f8ff}')
}

fn update_shipbuilding_hover_tooltip(
    mut commands: Commands,
    active_menu: Res<ActiveMenu>,
    ui_state: Res<ShipbuildingUiState>,
    shipbuilding_data: Res<ShipbuildingData>,
    research_state: Res<ResearchState>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    tooltip_body_children: Query<&Children>,
    mut tooltip_node: Single<&mut Node, With<ShipbuildingHoverTooltip>>,
    mut tooltip_title: Single<
        &mut Text,
        (
            With<ShipbuildingHoverTooltipTitle>,
            Without<ShipbuildingHoverTooltipBody>,
        ),
    >,
    tooltip_body: Single<
        Entity,
        (
            With<ShipbuildingHoverTooltipBody>,
            Without<ShipbuildingHoverTooltipTitle>,
        ),
    >,
) {
    if active_menu.current != GameMenu::Shipbuilding {
        tooltip_node.display = Display::None;
        return;
    }

    let Ok(window) = primary_window.single() else {
        tooltip_node.display = Display::None;
        return;
    };

    let Some(cursor) = window.cursor_position() else {
        tooltip_node.display = Display::None;
        return;
    };

    if let Some(module_id) = ui_state.hovered_module_id.as_deref() {
        if let Some(module) = shipbuilding_data.get_module(module_id) {
            let hovered_slot = ui_state.hovered_slot.as_deref().and_then(|slot_id| {
                ui_state
                    .selected_hull_id
                    .as_deref()
                    .and_then(|hull_id| shipbuilding_data.get_hull(hull_id))
                    .and_then(|hull| hull.slot_layout.iter().find(|slot| slot.slot_id == slot_id))
            });
            let content = build_module_tooltip(module, hovered_slot);
            let max_left = (window.width() - 360.0).max(0.0);
            let max_top = (window.height() - 300.0).max(0.0);
            let title = content.title.clone();
            populate_native_tooltip_body(
                &mut commands,
                *tooltip_body,
                &tooltip_body_children,
                &content,
            );
            tooltip_node.display = Display::Flex;
            tooltip_node.left = Val::Px((cursor.x + 16.0).min(max_left));
            tooltip_node.top = Val::Px((cursor.y + 12.0).min(max_top));
            **tooltip_title = Text::new(title);
            return;
        }
    }

    if let Some(slot_id) = ui_state.hovered_slot.as_deref() {
        if let Some(hull_id) = ui_state.selected_hull_id.as_deref() {
            if let Some(hull) = shipbuilding_data.get_hull(hull_id) {
                if let Some(slot) = hull.slot_layout.iter().find(|slot| slot.slot_id == slot_id) {
                    let compatible_modules =
                        shipbuilding_data.compatible_modules_for_slot(slot, &research_state);
                    let installed_module = ui_state
                        .selected_modules
                        .get(slot_id)
                        .and_then(|module_id| shipbuilding_data.get_module(module_id));
                    let content = build_slot_tooltip(slot, installed_module, &compatible_modules);
                    let max_left = (window.width() - 360.0).max(0.0);
                    let max_top = (window.height() - 300.0).max(0.0);
                    let title = content.title.clone();
                    populate_native_tooltip_body(
                        &mut commands,
                        *tooltip_body,
                        &tooltip_body_children,
                        &content,
                    );
                    tooltip_node.display = Display::Flex;
                    tooltip_node.left = Val::Px((cursor.x + 16.0).min(max_left));
                    tooltip_node.top = Val::Px((cursor.y + 12.0).min(max_top));
                    **tooltip_title = Text::new(title);
                    return;
                }
            }
        }
    }

    tooltip_node.display = Display::None;
}

fn populate_native_tooltip_body(
    commands: &mut Commands,
    body_entity: Entity,
    tooltip_body_children: &Query<&Children>,
    content: &ShipbuildingTooltipContent,
) {
    if let Ok(children) = tooltip_body_children.get(body_entity) {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
    commands.entity(body_entity).with_children(|parent| {
        for entry in &content.entries {
            match entry {
                ShipbuildingTooltipEntry::Paragraph(text) => {
                    parent.spawn((
                        Text::new(text.clone()),
                        TextFont {
                            font_size: 10.0,
                            ..default()
                        },
                        TextColor(tone_color_native(ShipbuildingTooltipTone::Muted)),
                    ));
                }
                ShipbuildingTooltipEntry::Stat { label, value, tone } => {
                    parent
                        .spawn((Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(6.0),
                            ..default()
                        },))
                        .with_children(|row| {
                            row.spawn((
                                Text::new(format!("{}:", label)),
                                TextFont {
                                    font_size: 10.5,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.86, 0.93, 0.98)),
                            ));
                            row.spawn((
                                Text::new(value.clone()),
                                TextFont {
                                    font_size: 10.0,
                                    ..default()
                                },
                                TextColor(tone_color_native(*tone)),
                            ));
                        });
                }
                ShipbuildingTooltipEntry::Spacer => {
                    parent.spawn((Node {
                        width: Val::Px(1.0),
                        height: Val::Px(4.0),
                        ..default()
                    },));
                }
            }
        }
    });
}

fn tone_color_native(tone: ShipbuildingTooltipTone) -> Color {
    match tone {
        ShipbuildingTooltipTone::Neutral => Color::srgb(0.84, 0.9, 0.94),
        ShipbuildingTooltipTone::Positive => Color::srgb(0.5, 0.92, 0.62),
        ShipbuildingTooltipTone::Warning => Color::srgb(0.98, 0.78, 0.36),
        ShipbuildingTooltipTone::Negative => Color::srgb(1.0, 0.45, 0.4),
        ShipbuildingTooltipTone::Accent => Color::srgb(0.55, 0.95, 1.0),
        ShipbuildingTooltipTone::Muted => Color::srgb(0.66, 0.75, 0.8),
    }
}

fn populate_blueprint_panel(
    commands: &mut Commands,
    blueprint_root: Entity,
    selected_hull: Option<&crate::shipbuilding::ShipHullDefinition>,
    ui_state: &ShipbuildingUiState,
    shipbuilding_data: &ShipbuildingData,
) {
    commands.entity(blueprint_root).with_children(|parent| {
        let Some(hull) = selected_hull else {
            parent.spawn(text_block(
                "No hull selected yet. The native blueprint becomes active once a hull is chosen in the existing ship design workflow.".to_string(),
                14.0,
                Color::srgb(0.82, 0.87, 0.9),
            ));
            return;
        };

        parent.spawn(text_block(
            format!(
                "{} • {:?} • {} slots • Installed modules: {}",
                hull.display_name,
                hull.class,
                hull.slot_layout.len(),
                ui_state.selected_modules.len(),
            ),
            15.0,
            Color::srgb(0.82, 0.87, 0.9),
        ));

        parent
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    position_type: PositionType::Relative,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.018, 0.032, 0.055, 0.94)),
                BorderColor::all(Color::srgb(0.15, 0.78, 0.88)),
            ))
            .with_children(|canvas| {
                spawn_blueprint_guides(canvas);

                for (slot, left, top, width, height) in derived_slot_layout(hull) {
                    let installed_module = ui_state
                        .selected_modules
                        .get(&slot.slot_id)
                        .and_then(|module_id| shipbuilding_data.get_module(module_id));
                    let is_selected = ui_state.selected_slot.as_deref() == Some(slot.slot_id.as_str());
                    let is_previewed = ui_state.preview_slot.as_deref() == Some(slot.slot_id.as_str());
                    let is_hovered = ui_state.hovered_slot.as_deref() == Some(slot.slot_id.as_str());
                    spawn_blueprint_slot(
                        canvas,
                        slot,
                        installed_module,
                        is_selected,
                        is_previewed,
                        is_hovered,
                        left,
                        top,
                        width,
                        height,
                    );
                }
            });
    });
}

fn populate_analytics_panel(
    commands: &mut Commands,
    analytics_root: Entity,
    selected_hull: Option<&crate::shipbuilding::ShipHullDefinition>,
    current_summary: Option<&ShipDesignSummary>,
    preview_summary: Option<&ShipDesignSummary>,
    ui_state: &ShipbuildingUiState,
) {
    commands.entity(analytics_root).with_children(|parent| {
        if selected_hull.is_none() {
            parent.spawn(text_block(
                "No design summary available yet. Once a hull and slots are selected, this panel will show live engineering metrics and bar-driven capacity usage.".to_string(),
                14.0,
                Color::srgb(0.82, 0.87, 0.9),
            ));
            return;
        }

        if let Some(summary) = current_summary {
            let preview = preview_summary.unwrap_or(summary);
            spawn_analytics_gauge(
                parent,
                "DV",
                "Delta-V",
                summary.delta_v_ms,
                preview.delta_v_ms,
                gauge_capacity(summary.delta_v_ms, preview.delta_v_ms, 100.0),
                "m/s",
                Color::srgb(0.0, 0.95, 1.0),
            );
            spawn_analytics_gauge(
                parent,
                "THR",
                "Thrust",
                summary.thrust_kn,
                preview.thrust_kn,
                gauge_capacity(summary.thrust_kn, preview.thrust_kn, 10.0),
                "kN",
                Color::srgb(0.96, 0.54, 0.28),
            );
            spawn_analytics_gauge(
                parent,
                "MASS",
                "Launch Mass",
                summary.launch_mass_t,
                preview.launch_mass_t,
                gauge_capacity(summary.launch_mass_t, preview.launch_mass_t, 10.0),
                "t",
                Color::srgb(0.5, 0.86, 1.0),
            );
            spawn_analytics_gauge(
                parent,
                "ACC",
                "Acceleration",
                summary.acceleration_ms2,
                preview.acceleration_ms2,
                gauge_capacity(summary.acceleration_ms2, preview.acceleration_ms2, 0.1),
                "m/s^2",
                Color::srgb(0.66, 0.96, 0.7),
            );
            spawn_analytics_gauge(
                parent,
                "PWR",
                "Net Power",
                summary.power_balance_mw(),
                preview.power_balance_mw(),
                gauge_capacity(summary.power_balance_mw(), preview.power_balance_mw(), 5.0),
                "MW",
                Color::srgb(0.0, 0.95, 1.0),
            );
            spawn_analytics_gauge(
                parent,
                "HTK",
                "Heat Sink",
                summary.heat_sink_capacity,
                preview.heat_sink_capacity,
                gauge_capacity(summary.heat_sink_capacity, preview.heat_sink_capacity, 5.0),
                "cap",
                Color::srgb(1.0, 0.64, 0.24),
            );
            spawn_analytics_gauge(
                parent,
                "SNS",
                "Sensor Range",
                summary.sensor_range_au,
                preview.sensor_range_au,
                gauge_capacity(summary.sensor_range_au, preview.sensor_range_au, 0.1),
                "AU",
                Color::srgb(0.5, 0.92, 0.9),
            );
            spawn_analytics_gauge(
                parent,
                "BLD",
                "Build Points",
                summary.build_points,
                preview.build_points,
                gauge_capacity(summary.build_points, preview.build_points, 50.0),
                "BP",
                Color::srgb(0.5, 0.92, 0.58),
            );
            spawn_analytics_gauge(
                parent,
                "FUEL",
                "Fuel Capacity",
                summary.fuel_capacity_t,
                preview.fuel_capacity_t,
                gauge_capacity(summary.fuel_capacity_t, preview.fuel_capacity_t, 5.0),
                "t",
                Color::srgb(0.45, 0.85, 0.66),
            );
            spawn_analytics_gauge(
                parent,
                "CARG",
                "Cargo Capacity",
                summary.cargo_capacity_t,
                preview.cargo_capacity_t,
                gauge_capacity(summary.cargo_capacity_t, preview.cargo_capacity_t, 5.0),
                "t",
                Color::srgb(0.86, 0.82, 0.58),
            );

            spawn_analytics_chip_row(
                parent,
                &[
                    (
                        "CREW",
                        format!("{:.0}", summary.crew),
                        format_delta(preview.crew - summary.crew, 0),
                        Color::srgb(0.86, 0.82, 0.58),
                    ),
                    (
                        "DOCK",
                        format!("{:.0}", summary.docking_ports),
                        format_delta(preview.docking_ports - summary.docking_ports, 0),
                        Color::srgb(0.82, 0.9, 0.96),
                    ),
                    (
                        "ISRU",
                        format!("{:.1}", summary.isru_rate_t_per_year),
                        format_delta(preview.isru_rate_t_per_year - summary.isru_rate_t_per_year, 1),
                        Color::srgb(0.9, 0.72, 0.45),
                    ),
                ],
            );

            spawn_analytics_chip_row(
                parent,
                &[
                    (
                        "GEN",
                        format!("{:.1} MW", summary.power_generation_mw),
                        format_delta(preview.power_generation_mw - summary.power_generation_mw, 1),
                        Color::srgb(0.0, 0.95, 1.0),
                    ),
                    (
                        "LOAD",
                        format!("{:.1} MW", summary.power_draw_mw),
                        format_delta(preview.power_draw_mw - summary.power_draw_mw, 1),
                        Color::srgb(1.0, 0.64, 0.24),
                    ),
                    (
                        "ORD",
                        format!("{:.1} t", summary.ordnance_capacity_t + summary.magazine_capacity_t),
                        String::new(),
                        Color::srgb(1.0, 0.46, 0.35),
                    ),
                ],
            );

            parent.spawn(text_block(
                format!(
                    "Preview module: {}",
                    ui_state
                        .preview_module_id
                        .clone()
                        .unwrap_or_else(|| "None".to_string())
                ),
                10.5,
                Color::srgb(0.82, 0.87, 0.9),
            ));

            if !summary.missing_required_slots.is_empty() {
                parent.spawn(text_block(
                    format!(
                        "Missing required slots: {}",
                        summary.missing_required_slots.join(", ")
                    ),
                    13.0,
                    Color::srgb(1.0, 0.55, 0.45),
                ));
            }

            let material_lines =
                format_shipbuilding_resource_cost_lines(&summary.resource_costs, 6).join("\n");
            parent.spawn(text_block(
                format!(
                    "Material Cost\n{}{}",
                    material_lines,
                    if summary.resource_costs.len() > 6 {
                        format!("\n+ {} more", summary.resource_costs.len() - 6)
                    } else {
                        String::new()
                    }
                ),
                10.5,
                Color::srgb(0.82, 0.87, 0.9),
            ));
        }
    });
}

fn animate_shipbuilding_slot_feedback(
    active_menu: Res<ActiveMenu>,
    ui_state: Res<ShipbuildingUiState>,
    time: Res<Time<Real>>,
    mut glows: Query<
        (&ShipbuildingSlotGlow, &mut Node, &mut BackgroundColor),
        (
            Without<ShipbuildingSlotFrame>,
            Without<ShipbuildingSlotAccentRail>,
        ),
    >,
    mut rails: Query<
        (&ShipbuildingSlotAccentRail, &mut Node, &mut BackgroundColor),
        (
            Without<ShipbuildingSlotGlow>,
            Without<ShipbuildingSlotFrame>,
        ),
    >,
    mut runners: Query<
        (
            &ShipbuildingSlotBorderRunner,
            &mut Node,
            &mut BackgroundColor,
        ),
        (
            Without<ShipbuildingSlotGlow>,
            Without<ShipbuildingSlotFrame>,
            Without<ShipbuildingSlotAccentRail>,
        ),
    >,
    mut frames: Query<
        (
            &ShipbuildingSlotFrame,
            &mut Node,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        (
            Without<ShipbuildingSlotGlow>,
            Without<ShipbuildingSlotAccentRail>,
        ),
    >,
) {
    if active_menu.current != GameMenu::Shipbuilding {
        return;
    }

    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 2.0).sin();
    let blend = (time.delta_secs() * 12.0).clamp(0.0, 1.0);
    let mut slot_dimensions = HashMap::new();

    for (glow, mut node, mut background) in &mut glows {
        let is_selected = ui_state.selected_slot.as_deref() == Some(glow.slot_id.as_str());
        let is_hovered = ui_state.hovered_slot.as_deref() == Some(glow.slot_id.as_str());
        let target_alpha = if is_selected {
            0.18 + pulse * 0.12
        } else if is_hovered {
            0.1 + pulse * 0.06
        } else if glow.filled {
            0.045
        } else {
            0.018
        };
        let width_boost = if is_selected {
            12.0
        } else if is_hovered {
            6.0
        } else {
            0.0
        };
        let height_boost = if is_selected {
            8.0
        } else if is_hovered {
            4.0
        } else {
            0.0
        };

        node.width = animate_px(node.width, glow.base_width + width_boost, blend);
        node.height = animate_px(node.height, glow.base_height + height_boost, blend);
        background.0 = mix_color(background.0, glow.accent.with_alpha(target_alpha), blend);

        slot_dimensions.insert(
            glow.slot_id.clone(),
            (
                val_px(node.width, glow.base_width),
                val_px(node.height, glow.base_height),
            ),
        );
    }

    for (rail, mut node, mut background) in &mut rails {
        let is_selected = ui_state.selected_slot.as_deref() == Some(rail.slot_id.as_str());
        let is_hovered = ui_state.hovered_slot.as_deref() == Some(rail.slot_id.as_str());
        let target_width = if is_selected {
            7.0
        } else if is_hovered {
            5.0
        } else {
            3.0
        };
        let target_alpha = if is_selected {
            0.98
        } else if is_hovered {
            0.88
        } else {
            0.74
        };

        node.width = animate_px(node.width, target_width, blend);
        background.0 = mix_color(background.0, rail.accent.with_alpha(target_alpha), blend);
    }

    for (runner, mut node, mut background) in &mut runners {
        let is_selected = ui_state.selected_slot.as_deref() == Some(runner.slot_id.as_str());
        let is_hovered = ui_state.hovered_slot.as_deref() == Some(runner.slot_id.as_str());
        let visible = is_selected || is_hovered;
        let (width, height) = slot_dimensions
            .get(&runner.slot_id)
            .copied()
            .unwrap_or((runner.width, runner.height));
        let speed = if is_selected {
            26.0
        } else if is_hovered {
            18.0
        } else {
            0.0
        };
        let alpha = if is_selected {
            0.8 + pulse * 0.16
        } else if is_hovered {
            0.46 + pulse * 0.08
        } else {
            0.0
        };
        let perimeter = border_runner_perimeter(width, height, runner.size);
        let distance = ((time.elapsed_secs() * speed) + runner.phase_offset * 128.0
            - runner.trail_offset)
            .rem_euclid(perimeter);
        let (left, top) = border_runner_point(distance, width, height, runner.size);

        node.display = if visible {
            Display::Flex
        } else {
            Display::None
        };
        node.left = Val::Px(left);
        node.top = Val::Px(top);
        node.width = Val::Px(runner.size);
        node.height = Val::Px(runner.size);
        background.0 = runner.accent.with_alpha(alpha * runner.alpha_scale);
    }

    for (frame, mut node, mut background, mut border) in &mut frames {
        let is_selected = ui_state.selected_slot.as_deref() == Some(frame.slot_id.as_str());
        let is_hovered = ui_state.hovered_slot.as_deref() == Some(frame.slot_id.as_str());
        let mut fill = if frame.filled {
            Color::srgb(0.045, 0.12, 0.15)
        } else {
            Color::srgba(0.018, 0.032, 0.05, 0.94)
        };

        node.border = UiRect::all(animate_px(
            node.border.left,
            if is_selected {
                3.0
            } else if is_hovered {
                2.0
            } else {
                1.0
            },
            blend,
        ));

        if is_selected {
            fill = mix_color(fill, frame.accent, 0.22 + pulse * 0.1);
            *border = BorderColor::all(mix_color(
                Color::srgb(0.55, 0.95, 1.0),
                frame.accent,
                0.62 + pulse * 0.18,
            ));
        } else if is_hovered {
            fill = mix_color(fill, frame.accent, 0.14 + pulse * 0.05);
            *border =
                BorderColor::all(mix_color(Color::srgb(0.36, 0.88, 0.98), frame.accent, 0.38));
        } else if frame.previewed {
            *border = BorderColor::all(Color::srgb(0.46, 0.78, 1.0));
        } else if frame.filled {
            *border =
                BorderColor::all(mix_color(Color::srgb(0.28, 0.72, 0.82), frame.accent, 0.18));
        } else {
            *border = BorderColor::all(Color::srgb(0.22, 0.45, 0.54));
        }

        background.0 = fill;
    }
}

fn animate_shipbuilding_module_card_feedback(
    active_menu: Res<ActiveMenu>,
    ui_state: Res<ShipbuildingUiState>,
    time: Res<Time<Real>>,
    mut cards: Query<
        (
            &Interaction,
            &ShipbuildingModuleCard,
            &mut Node,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<Button>,
    >,
) {
    if active_menu.current != GameMenu::Shipbuilding {
        return;
    }

    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 2.4).sin();
    let blend = (time.delta_secs() * 14.0).clamp(0.0, 1.0);
    for (interaction, card, mut node, mut background, mut border) in &mut cards {
        let installed = ui_state.selected_modules.get(&card.slot_id) == Some(&card.module_id);
        let previewed = ui_state.preview_slot.as_deref() == Some(card.slot_id.as_str())
            && ui_state.preview_module_id.as_deref() == Some(card.module_id.as_str());
        let hovered = *interaction == Interaction::Hovered;

        node.height = animate_px(
            node.height,
            card.base_height
                + if installed {
                    2.0
                } else if hovered {
                    4.0
                } else {
                    0.0
                },
            blend,
        );

        background.0 = if installed {
            mix_color(
                Color::srgb(0.1, 0.32, 0.22),
                Color::srgb(0.38, 0.94, 0.7),
                0.08 + pulse * 0.06,
            )
        } else if hovered {
            Color::srgb(0.08, 0.13, 0.19)
        } else if previewed {
            Color::srgb(0.1, 0.18, 0.28)
        } else {
            Color::srgb(0.055, 0.08, 0.12)
        };

        *border = BorderColor::all(if installed {
            Color::srgb(0.38, 0.94, 0.7)
        } else if hovered {
            Color::srgb(0.55, 0.95, 1.0)
        } else if previewed {
            Color::srgb(0.46, 0.78, 1.0)
        } else {
            Color::srgb(0.22, 0.35, 0.42)
        });
    }
}

fn build_preview_summary(
    ui_state: &ShipbuildingUiState,
    shipbuilding_data: &ShipbuildingData,
    research_state: &ResearchState,
) -> Option<ShipDesignSummary> {
    let mut design = build_preview_design(ui_state)?;

    let Some(slot_id) = ui_state.preview_slot.as_deref() else {
        return shipbuilding_data.summarize_design(&design, research_state);
    };
    let Some(module_id) = ui_state.preview_module_id.as_deref() else {
        return shipbuilding_data.summarize_design(&design, research_state);
    };

    if let Some(existing) = design
        .modules
        .iter_mut()
        .find(|selection| selection.slot_id == slot_id)
    {
        existing.module_id = module_id.to_string();
    } else {
        design.modules.push(ShipModuleSelection {
            slot_id: slot_id.to_string(),
            module_id: module_id.to_string(),
        });
    }

    shipbuilding_data.summarize_design(&design, research_state)
}

fn active_slot<'a>(
    hull: &'a crate::shipbuilding::ShipHullDefinition,
    ui_state: &ShipbuildingUiState,
) -> Option<&'a HullSlotDefinition> {
    if let Some(slot_id) = ui_state.selected_slot.as_deref() {
        hull.slot_layout.iter().find(|slot| slot.slot_id == slot_id)
    } else {
        hull.slot_layout.first()
    }
}

fn derived_slot_layout(
    hull: &crate::shipbuilding::ShipHullDefinition,
) -> Vec<(&HullSlotDefinition, f32, f32, f32, f32)> {
    let zone_totals = zone_totals(hull);
    let mut zone_indices = [0_u32; 8];
    let mut slots = Vec::with_capacity(hull.slot_layout.len());

    for slot in &hull.slot_layout {
        let zone = slot_zone(slot);
        let total = zone_totals[zone].max(1);
        let index = zone_indices[zone];
        zone_indices[zone] += 1;

        let (width, height) = slot_dimensions(&slot.size);
        let left = if let Some((x, _)) = slot.position {
            (x * 82.0).clamp(4.0, 90.0)
        } else {
            zone_left_percent(zone, slot, index, total)
        };
        let top = if let Some((_, y)) = slot.position {
            ((1.0 - y) * 78.0).clamp(8.0, 82.0)
        } else {
            zone_top_percent(zone, index, total, slot)
        };

        slots.push((slot, left, top, width, height));
    }

    slots
}

fn zone_totals(hull: &crate::shipbuilding::ShipHullDefinition) -> [u32; 8] {
    let mut totals = [0_u32; 8];
    for slot in &hull.slot_layout {
        totals[slot_zone(slot)] += 1;
    }
    totals
}

fn slot_zone(slot: &HullSlotDefinition) -> usize {
    let slot_id = slot.slot_id.to_ascii_lowercase();

    if slot_id.contains("drive") || slot_id.contains("engine") || slot_id.contains("thruster") {
        return 0;
    }
    if slot_id.contains("tank")
        || slot_id.contains("cargo")
        || slot_id.contains("fuel")
        || slot_id.contains("storage")
    {
        return 1;
    }
    if slot_id.contains("reactor")
        || slot_id.contains("power")
        || slot_id.contains("battery")
        || slot_id.contains("heat")
        || slot_id.contains("radiator")
    {
        return 2;
    }
    if slot_id.contains("sensor")
        || slot_id.contains("mission")
        || slot_id.contains("utility")
        || slot_id.contains("aux")
        || slot_id.contains("support")
    {
        return 3;
    }
    if slot_id.contains("hangar")
        || slot_id.contains("bay")
        || slot_id.contains("isru")
        || slot_id.contains("mining")
        || slot_id.contains("construction")
    {
        return 4;
    }
    if slot_id.contains("command")
        || slot_id.contains("bridge")
        || slot_id.contains("cic")
        || slot_id.contains("crew")
    {
        return 5;
    }
    if slot_id.contains("armor")
        || slot_id.contains("magazine")
        || slot_id.contains("pd")
        || slot_id.contains("shield")
        || slot_id.contains("defense")
    {
        return 6;
    }
    if slot_id.contains("weapon")
        || slot_id.contains("spinal")
        || slot_id.contains("missile")
        || slot_id.contains("gun")
        || slot_id.contains("laser")
    {
        return 7;
    }

    match slot.category {
        ShipModuleCategory::FlightSystems => 0,
        ShipModuleCategory::FuelStorage | ShipModuleCategory::CargoStorage => 1,
        ShipModuleCategory::PowerThermal => 2,
        ShipModuleCategory::Sensors
        | ShipModuleCategory::UtilitySupport
        | ShipModuleCategory::Maintenance
        | ShipModuleCategory::SpecialScience
        | ShipModuleCategory::ElectronicWarfare => 3,
        ShipModuleCategory::ConstructionISRU | ShipModuleCategory::Construction => 4,
        ShipModuleCategory::CrewSystems
        | ShipModuleCategory::Bridges
        | ShipModuleCategory::Habitats
        | ShipModuleCategory::Medical => 5,
        ShipModuleCategory::ArmorDefense
        | ShipModuleCategory::Magazines
        | ShipModuleCategory::PointDefense
        | ShipModuleCategory::Armor => 6,
        ShipModuleCategory::Weapons | ShipModuleCategory::FireControl => 7,
    }
}

fn slot_dimensions(size: &str) -> (f32, f32) {
    match size {
        "Small" => (126.0, 64.0),
        "Medium" => (126.0, 68.0),
        "Large" => (126.0, 72.0),
        _ => (126.0, 66.0),
    }
}

fn spawn_blueprint_guides(parent: &mut ChildSpawnerCommands) {
    let sections = [
        (4.0, "Engines"),
        (15.6, "Fuel"),
        (27.2, "Power"),
        (38.8, "Support"),
        (50.4, "Industry"),
        (62.0, "Command"),
        (73.6, "Defense"),
        (85.2, "Weapons"),
    ];

    for (left, label) in sections {
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(left),
                top: Val::Percent(5.0),
                width: Val::Px(1.0),
                height: Val::Percent(89.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.18, 0.55, 0.64, 0.18)),
        ));

        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent((left + 0.8).min(92.0)),
                top: Val::Percent(1.5),
                ..default()
            },
            Text::new(label),
            TextFont {
                font_size: 9.5,
                ..default()
            },
            TextColor(Color::srgb(0.52, 0.8, 0.88)),
        ));
    }

    for top in [18.0, 38.0, 58.0, 78.0] {
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(4.0),
                top: Val::Percent(top),
                width: Val::Percent(92.0),
                height: Val::Px(1.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.18, 0.55, 0.64, 0.12)),
        ));
    }
}

fn zone_left_percent(zone: usize, slot: &HullSlotDefinition, index: u32, total: u32) -> f32 {
    let slot_id = slot.slot_id.to_ascii_lowercase();
    let columns = zone_columns(zone, total);
    let column = index % columns;
    let (start, end) = zone_bounds(zone);
    let span = (end - start).max(4.0);
    let step = span / columns as f32;
    let base = start + 0.8 + column as f32 * step;

    if zone == 7 && slot_id.contains("spinal") {
        return 86.0;
    }
    if zone == 7 && (slot_id.contains("port") || slot_id.contains("left")) {
        return 84.0;
    }
    if zone == 7 && (slot_id.contains("starboard") || slot_id.contains("right")) {
        return 88.0;
    }

    base
}

fn zone_top_percent(zone: usize, index: u32, total: u32, slot: &HullSlotDefinition) -> f32 {
    let columns = zone_columns(zone, total);
    let row = index / columns;
    let row_stride = 11.5;
    let base_row = 12.0 + row as f32 * row_stride;
    let slot_id = slot.slot_id.to_ascii_lowercase();

    let directional_bias = if slot_id.contains("port") || slot_id.contains("left") {
        -2.0
    } else if slot_id.contains("starboard") || slot_id.contains("right") {
        2.0
    } else {
        0.0
    };

    (base_row + directional_bias).clamp(10.0, 85.0)
}

fn zone_bounds(zone: usize) -> (f32, f32) {
    match zone {
        0 => (4.0, 15.0),
        1 => (15.6, 26.6),
        2 => (27.2, 38.2),
        3 => (38.8, 49.8),
        4 => (50.4, 61.4),
        5 => (62.0, 73.0),
        6 => (73.6, 84.6),
        _ => (85.2, 96.2),
    }
}

fn zone_columns(_zone: usize, _total: u32) -> u32 {
    1
}

fn spawn_blueprint_slot(
    parent: &mut ChildSpawnerCommands,
    slot: &HullSlotDefinition,
    installed_module: Option<&crate::shipbuilding::ShipModuleDefinition>,
    is_selected: bool,
    is_previewed: bool,
    is_hovered: bool,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
) {
    let filled = installed_module.is_some();
    let accent = theme::module_slot_accent_color(slot.category);
    let root_fill = if is_selected {
        accent.with_alpha(0.18)
    } else if is_hovered {
        accent.with_alpha(0.11)
    } else if filled {
        accent.with_alpha(0.045)
    } else {
        accent.with_alpha(0.02)
    };
    let frame_fill = if is_selected {
        mix_color(Color::srgb(0.04, 0.1, 0.14), accent, 0.26)
    } else if is_hovered {
        mix_color(Color::srgb(0.03, 0.08, 0.11), accent, 0.16)
    } else if filled {
        Color::srgb(0.045, 0.12, 0.15)
    } else {
        Color::srgba(0.018, 0.032, 0.05, 0.94)
    };
    let frame_border = if is_selected {
        mix_color(Color::srgb(0.55, 0.95, 1.0), accent, 0.72)
    } else if is_hovered {
        mix_color(Color::srgb(0.4, 0.9, 1.0), accent, 0.42)
    } else if is_previewed {
        Color::srgb(0.46, 0.78, 1.0)
    } else if filled {
        mix_color(Color::srgb(0.34, 0.86, 0.94), accent, 0.22)
    } else {
        Color::srgba(0.16, 0.34, 0.4, 0.45)
    };
    let title = prettify_slot_name(&slot.slot_id);
    let category_text = slot.category.display_name();
    let module_name = installed_module
        .map(|module| module.display_name.clone())
        .unwrap_or_else(|| {
            if slot.required {
                "SOCKET READY".to_string()
            } else {
                "AUX SOCKET".to_string()
            }
        });

    parent
        .spawn((
            Button,
            ShipbuildingSlotButton {
                slot_id: slot.slot_id.clone(),
            },
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(left),
                top: Val::Percent(top),
                width: Val::Px(width),
                height: Val::Px(height),
                padding: UiRect::all(Val::Px(0.0)),
                ..default()
            },
            ShipbuildingSlotGlow {
                slot_id: slot.slot_id.clone(),
                base_width: width,
                base_height: height,
                accent,
                filled,
            },
            BackgroundColor(root_fill),
        ))
        .with_children(|slot_root| {
            slot_root.spawn((
                ShipbuildingSlotAccentRail {
                    slot_id: slot.slot_id.clone(),
                    accent,
                },
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(3.0),
                    top: Val::Px(0.0),
                    width: Val::Px(if is_selected {
                        5.0
                    } else if is_hovered {
                        4.0
                    } else {
                        3.0
                    }),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(accent.with_alpha(if is_selected {
                    0.98
                } else if is_hovered {
                    0.88
                } else {
                    0.74
                })),
            ));

            slot_root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                ShipbuildingSlotFrame {
                    slot_id: slot.slot_id.clone(),
                    accent,
                    filled,
                    previewed: is_previewed,
                },
                BackgroundColor(frame_fill),
                BorderColor::all(frame_border),
            ));

            slot_root
                .spawn((Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(12.0),
                    top: Val::Px(6.0),
                    width: Val::Px((width - 20.0).max(96.0)),
                    height: Val::Px((height - 14.0).max(48.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(1.0),
                    ..default()
                },))
                .with_children(|content| {
                    content
                        .spawn((Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::SpaceBetween,
                            ..default()
                        },))
                        .with_children(|row| {
                            row.spawn((
                                Text::new(title),
                                TextFont {
                                    font_size: 9.5,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.88, 0.93, 0.96)),
                            ));
                            row.spawn((
                                Text::new(format!(
                                    "{} [{}]",
                                    slot.category.display_name(),
                                    size_badge(&slot.size)
                                )),
                                TextFont {
                                    font_size: 7.8,
                                    ..default()
                                },
                                TextColor(accent),
                            ));
                        });

                    content.spawn((
                        Text::new(if filled {
                            module_name
                        } else {
                            format!("{} socket", category_text)
                        }),
                        TextFont {
                            font_size: if filled { 8.2 } else { 7.4 },
                            ..default()
                        },
                        TextColor(if filled {
                            Color::srgb(0.78, 0.86, 0.9)
                        } else {
                            Color::srgba(0.7, 0.85, 0.92, 0.32)
                        }),
                    ));

                    content
                        .spawn((Node {
                            width: Val::Percent(100.0),
                            margin: UiRect::top(Val::Px(1.0)),
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::End,
                            ..default()
                        },))
                        .with_children(|row| {
                            row.spawn((
                                Text::new(micro_stats_text(slot, installed_module)),
                                TextFont {
                                    font_size: 7.2,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.82, 0.87, 0.9)),
                            ));
                            row.spawn((
                                Text::new(metric_corner_text(installed_module)),
                                TextFont {
                                    font_size: 7.2,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.78, 0.84, 0.88)),
                            ));
                        });
                });

            for (trail_offset, alpha_scale, size) in
                [(0.0, 1.0, 6.0), (8.0, 0.5, 4.5), (15.0, 0.24, 3.2)]
            {
                slot_root.spawn((
                    ShipbuildingSlotBorderRunner {
                        slot_id: slot.slot_id.clone(),
                        width,
                        height,
                        phase_offset: hash_phase(&slot.slot_id),
                        accent,
                        trail_offset,
                        alpha_scale,
                        size,
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(2.0),
                        top: Val::Px(2.0),
                        width: Val::Px(size),
                        height: Val::Px(size),
                        display: if is_selected || is_hovered {
                            Display::Flex
                        } else {
                            Display::None
                        },
                        ..default()
                    },
                    BackgroundColor(
                        accent.with_alpha(if is_selected { 0.86 } else { 0.5 } * alpha_scale),
                    ),
                ));
            }
        });
}

fn size_badge(size: &str) -> &'static str {
    match size {
        "Small" => "S",
        "Medium" => "M",
        "Large" => "L",
        _ => "A",
    }
}

fn micro_stats_text(
    slot: &HullSlotDefinition,
    module: Option<&crate::shipbuilding::ShipModuleDefinition>,
) -> String {
    let Some(module) = module else {
        return format!(
            "{} {}",
            if slot.required { "REQ" } else { "AUX" },
            pip_bar(0, 5)
        );
    };

    let power_metric = module
        .power_draw_mw
        .abs()
        .max(module.power_generation_mw.abs() * 0.5);
    let power_pips = pip_count(power_metric, [0.5, 2.0, 6.0, 15.0]);
    let heat_alert = if module.category == ShipModuleCategory::PowerThermal
        && module.power_generation_mw > 0.0
    {
        "HOT"
    } else {
        "OK"
    };

    format!("PWR {} THM {}", pip_bar(power_pips, 5), heat_alert)
}

fn metric_corner_text(module: Option<&crate::shipbuilding::ShipModuleDefinition>) -> String {
    let Some(module) = module else {
        return "MASS --  NET --".to_string();
    };
    format!(
        "MASS {:.0}t  NET {:+.0}",
        module.dry_mass_t,
        module.power_generation_mw - module.power_draw_mw
    )
}

fn pip_count(value: f64, thresholds: [f64; 4]) -> usize {
    let mut count = 1;
    for threshold in thresholds {
        if value >= threshold {
            count += 1;
        }
    }
    count.min(5)
}

fn pip_bar(active: usize, total: usize) -> String {
    (0..total)
        .map(|index| if index < active { '#' } else { '.' })
        .collect()
}

fn hash_phase(value: &str) -> f32 {
    let hash = value.bytes().fold(0_u32, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as u32)
    });
    (hash % 100) as f32 / 100.0
}

fn border_runner_perimeter(width: f32, height: f32, size: f32) -> f32 {
    let half = size * 0.5;
    let min_x = -half;
    let min_y = -half;
    let max_x = (width - half).max(min_x);
    let max_y = (height - half).max(min_y);

    ((max_x - min_x) * 2.0 + (max_y - min_y) * 2.0).max(1.0)
}

fn border_runner_point(distance: f32, width: f32, height: f32, size: f32) -> (f32, f32) {
    let half = size * 0.5;
    let min_x = -half;
    let min_y = -half;
    let max_x = (width - half).max(min_x);
    let max_y = (height - half).max(min_y);
    let top_length = (max_x - min_x).max(1.0);
    let side_length = (max_y - min_y).max(1.0);
    let perimeter = top_length * 2.0 + side_length * 2.0;
    let d = distance % perimeter;

    if d < top_length {
        (min_x + d, min_y)
    } else if d < top_length + side_length {
        (max_x, min_y + (d - top_length))
    } else if d < top_length * 2.0 + side_length {
        (max_x - (d - top_length - side_length), max_y)
    } else {
        (min_x, max_y - (d - top_length * 2.0 - side_length))
    }
}

fn val_px(value: Val, fallback: f32) -> f32 {
    match value {
        Val::Px(px) => px,
        _ => fallback,
    }
}

fn spawn_analytics_gauge(
    parent: &mut ChildSpawnerCommands,
    code: &str,
    label: &str,
    current: f64,
    preview: f64,
    capacity: f64,
    unit: &str,
    color: Color,
) {
    let current_pct = ((current.abs() / capacity) as f32).clamp(0.0, 1.0);
    let preview_pct = ((preview.abs() / capacity) as f32).clamp(0.0, 1.0);

    parent
        .spawn((Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(3.0),
            ..default()
        },))
        .with_children(|column| {
            column
                .spawn((Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(6.0),
                    align_items: AlignItems::Center,
                    ..default()
                },))
                .with_children(|row| {
                    row.spawn((
                        Node {
                            min_width: Val::Px(40.0),
                            padding: UiRect::axes(Val::Px(5.0), Val::Px(2.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(color.with_alpha(0.16)),
                        BorderColor::all(color),
                        Text::new(code),
                        TextFont {
                            font_size: 10.5,
                            ..default()
                        },
                        TextColor(color),
                    ));
                    row.spawn((
                        Text::new(format!(
                            "{}  {:.2} {}  {}",
                            label,
                            current,
                            unit,
                            format_delta(preview - current, 2)
                        )),
                        TextFont {
                            font_size: 10.5,
                            ..default()
                        },
                        TextColor(Color::srgb(0.84, 0.9, 0.94)),
                    ));
                });

            column
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(11.0),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.03, 0.05, 0.08, 0.98)),
                    BorderColor::all(Color::srgb(0.22, 0.35, 0.42)),
                ))
                .with_children(|track| {
                    track.spawn((
                        Node {
                            width: Val::Percent(current_pct * 100.0),
                            height: Val::Percent(100.0),
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(0.0),
                            ..default()
                        },
                        BackgroundColor(color),
                    ));

                    if preview_pct > current_pct {
                        track.spawn((
                            Node {
                                width: Val::Percent((preview_pct - current_pct) * 100.0),
                                height: Val::Percent(100.0),
                                position_type: PositionType::Absolute,
                                left: Val::Percent(current_pct * 100.0),
                                top: Val::Px(0.0),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.45, 0.92, 0.56, 0.55)),
                        ));
                    } else if preview_pct < current_pct {
                        track.spawn((
                            Node {
                                width: Val::Percent((current_pct - preview_pct) * 100.0),
                                height: Val::Percent(100.0),
                                position_type: PositionType::Absolute,
                                left: Val::Percent(preview_pct * 100.0),
                                top: Val::Px(0.0),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(1.0, 0.38, 0.3, 0.55)),
                        ));
                    }
                });
        });
}

fn spawn_analytics_chip_row(
    parent: &mut ChildSpawnerCommands,
    chips: &[(&str, String, String, Color)],
) {
    parent
        .spawn((Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(4.0),
            ..default()
        },))
        .with_children(|row| {
            for (code, value, delta, color) in chips {
                row.spawn((
                    Node {
                        flex_grow: 1.0,
                        padding: UiRect::all(Val::Px(5.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        ..default()
                    },
                    BackgroundColor(color.with_alpha(0.12)),
                    BorderColor::all(*color),
                ))
                .with_children(|chip| {
                    chip.spawn(text_block((*code).to_string(), 10.5, *color));
                    chip.spawn(text_block(
                        value.clone(),
                        10.5,
                        Color::srgb(0.84, 0.9, 0.94),
                    ));
                    if !delta.is_empty() {
                        chip.spawn(text_block(
                            delta.clone(),
                            10.5,
                            Color::srgb(0.68, 0.9, 0.76),
                        ));
                    }
                });
            }
        });
}

fn gauge_capacity(current: f64, preview: f64, floor: f64) -> f64 {
    current.abs().max(preview.abs()).max(floor) * 1.15
}

fn format_delta(delta: f64, decimals: usize) -> String {
    match decimals {
        0 => format!("{:+.0}", delta),
        1 => format!("{:+.1}", delta),
        _ => format!("{:+.2}", delta),
    }
}

fn mix_color(base: Color, target: Color, amount: f32) -> Color {
    let base = base.to_srgba();
    let target = target.to_srgba();
    Color::srgba(
        base.red + (target.red - base.red) * amount,
        base.green + (target.green - base.green) * amount,
        base.blue + (target.blue - base.blue) * amount,
        base.alpha + (target.alpha - base.alpha) * amount,
    )
}

fn animate_px(value: Val, target: f32, amount: f32) -> Val {
    let current = match value {
        Val::Px(px) => px,
        _ => target,
    };
    Val::Px(current + (target - current) * amount)
}

fn text_block(text: String, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: size,
            ..default()
        },
        TextColor(color),
    )
}

fn spawn_hull_controls(
    parent: &mut ChildSpawnerCommands,
    available_hulls: &[&crate::shipbuilding::ShipHullDefinition],
    selected_hull: Option<&crate::shipbuilding::ShipHullDefinition>,
    ui_state: &ShipbuildingUiState,
) {
    parent.spawn(text_block(
        "Hull Control".to_string(),
        13.0,
        Color::srgb(0.55, 0.95, 1.0),
    ));
    parent.spawn(text_block(
        format!(
            "Current hull: {}",
            selected_hull
                .map(|hull| hull.display_name.as_str())
                .unwrap_or("None")
        ),
        11.5,
        Color::srgb(0.82, 0.87, 0.9),
    ));
    parent.spawn(dropdown_toggle_button(
        selected_hull
            .map(|hull| format!("Hull: {}", hull.display_name))
            .unwrap_or_else(|| "Select Hull".to_string()),
        ui_state.show_hull_dropdown,
    ));

    if ui_state.show_hull_dropdown {
        parent
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    padding: UiRect::all(Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.045, 0.07, 0.1)),
                BorderColor::all(Color::srgb(0.22, 0.48, 0.58)),
            ))
            .with_children(|column| {
                for hull in available_hulls {
                    let is_selected = selected_hull.is_some_and(|selected| selected.id == hull.id);
                    column.spawn(hull_option_button(
                        hull.id.clone(),
                        hull.display_name.clone(),
                        is_selected,
                    ));
                }
            });
    }

    if ui_state.selected_hull_id.is_none() {
        parent.spawn(text_block(
            "Use the hull controls to seed the native workspace without returning to the legacy designer.".to_string(),
            10.5,
            Color::srgb(0.6, 0.7, 0.76),
        ));
    }
}

fn spawn_category_controls(
    parent: &mut ChildSpawnerCommands,
    selected_hull: Option<&crate::shipbuilding::ShipHullDefinition>,
    ui_state: &ShipbuildingUiState,
) {
    let Some(hull) = selected_hull else {
        return;
    };

    parent.spawn(text_block(
        "Category Navigation".to_string(),
        13.0,
        Color::srgb(0.55, 0.95, 1.0),
    ));

    let present_categories: Vec<_> = ShipModuleCategory::all()
        .iter()
        .copied()
        .filter(|category| {
            hull.slot_layout
                .iter()
                .any(|slot| slot.category == *category)
        })
        .collect();

    parent
        .spawn((Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            ..default()
        },))
        .with_children(|rows| {
            for chunk in present_categories.chunks(2) {
                rows.spawn((Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(4.0),
                    ..default()
                },))
                    .with_children(|row| {
                        for category in chunk {
                            let selected = ui_state.selected_slot.as_deref().and_then(|slot_id| {
                                hull.slot_layout
                                    .iter()
                                    .find(|slot| slot.slot_id == slot_id)
                                    .map(|slot| slot.category)
                            }) == Some(*category);

                            row.spawn(category_button(*category, selected));
                        }

                        if chunk.len() == 1 {
                            row.spawn((Node {
                                flex_grow: 1.0,
                                ..default()
                            },));
                        }
                    });
            }
        });
}

fn dropdown_toggle_button(label: String, open: bool) -> impl Bundle {
    (
        Button,
        ShipbuildingHullDropdownToggle,
        Node {
            width: Val::Percent(100.0),
            min_height: Val::Px(28.0),
            padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.06, 0.11, 0.16)),
        BorderColor::all(if open {
            Color::srgb(0.0, 0.95, 1.0)
        } else {
            Color::srgb(0.22, 0.48, 0.58)
        }),
        Text::new(format!("{} {}", if open { "▾" } else { "▸" }, label)),
        TextFont {
            font_size: 11.5,
            ..default()
        },
        TextColor(Color::srgb(0.88, 0.93, 0.96)),
    )
}

fn hull_option_button(hull_id: String, label: String, selected: bool) -> impl Bundle {
    (
        Button,
        ShipbuildingHullOptionButton { hull_id },
        Node {
            width: Val::Percent(100.0),
            min_height: Val::Px(24.0),
            padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(if selected {
            Color::srgb(0.1, 0.18, 0.22)
        } else {
            Color::srgb(0.045, 0.07, 0.1)
        }),
        BorderColor::all(if selected {
            Color::srgb(0.0, 0.95, 1.0)
        } else {
            Color::srgb(0.18, 0.3, 0.36)
        }),
        Text::new(label),
        TextFont {
            font_size: 11.0,
            ..default()
        },
        TextColor(Color::srgb(0.88, 0.93, 0.96)),
    )
}

fn category_button(category: ShipModuleCategory, selected: bool) -> impl Bundle {
    (
        Button,
        ShipbuildingCategoryButton { category },
        Node {
            flex_grow: 1.0,
            min_height: Val::Px(24.0),
            padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(if selected {
            Color::srgb(0.1, 0.18, 0.22)
        } else {
            Color::srgb(0.055, 0.08, 0.12)
        }),
        BorderColor::all(if selected {
            theme::module_category_color(category)
        } else {
            Color::srgb(0.22, 0.35, 0.42)
        }),
        Text::new(category.display_name()),
        TextFont {
            font_size: 10.5,
            ..default()
        },
        TextColor(Color::srgb(0.88, 0.93, 0.96)),
    )
}

fn select_hull_by_id(
    ui_state: &mut ShipbuildingUiState,
    shipbuilding_data: &ShipbuildingData,
    research_state: &ResearchState,
    hull_id: &str,
) {
    let Some(hull) = shipbuilding_data.get_hull(hull_id) else {
        return;
    };
    if !shipbuilding_data.hull_is_unlocked(hull, research_state) {
        return;
    }

    ui_state.selected_hull_id = Some(hull.id.clone());
    ui_state.selected_template_id = None;
    ui_state.upgrade_source_template_id = None;
    ui_state.selected_mode = hull.default_construction_mode;
    ui_state.design_name = format!("{} Prototype", hull.display_name);
    ui_state.selected_modules.clear();
    ui_state.selected_slot = hull.slot_layout.first().map(|slot| slot.slot_id.clone());
    ui_state.preview_slot = None;
    ui_state.preview_module_id = None;
    ui_state.show_hull_dropdown = false;
    hydrate_selected_design_native(ui_state, shipbuilding_data, research_state);
}

fn select_first_slot_in_category(
    ui_state: &mut ShipbuildingUiState,
    shipbuilding_data: &ShipbuildingData,
    category: ShipModuleCategory,
) {
    let Some(hull_id) = ui_state.selected_hull_id.as_deref() else {
        return;
    };
    let Some(hull) = shipbuilding_data.get_hull(hull_id) else {
        return;
    };

    if let Some(slot) = hull
        .slot_layout
        .iter()
        .find(|slot| slot.category == category)
    {
        ui_state.selected_slot = Some(slot.slot_id.clone());
        ui_state.preview_slot = None;
        ui_state.preview_module_id = None;
    }
}

fn hydrate_selected_design_native(
    ui_state: &mut ShipbuildingUiState,
    shipbuilding_data: &ShipbuildingData,
    research_state: &ResearchState,
) {
    let Some(hull_id) = ui_state.selected_hull_id.as_deref() else {
        return;
    };
    let Some(hull) = shipbuilding_data.get_hull(hull_id) else {
        return;
    };

    for slot in &hull.slot_layout {
        if ui_state.selected_modules.contains_key(&slot.slot_id) {
            continue;
        }

        if let Some(module) = shipbuilding_data
            .compatible_modules_for_slot(slot, research_state)
            .first()
        {
            ui_state
                .selected_modules
                .insert(slot.slot_id.clone(), module.id.clone());
        }
    }
}

struct EngineeringStatusNative {
    label: &'static str,
    color: Color,
}

fn populate_archive_tab_native(
    commands: &mut Commands,
    library_root: Entity,
    blueprint_root: Entity,
    analytics_root: Entity,
    colonies: &WorkspaceColonyQuery,
    ships: &WorkspaceShipQuery,
    refits: &Query<(Entity, &RefitProject)>,
    design_library: &ShipDesignLibrary,
    shipbuilding_data: &ShipbuildingData,
    research_state: &ResearchState,
    ui_state: &ShipbuildingUiState,
) {
    let _available_sorts = [
        super::shipbuilding_state::DesignSort::HullType,
        super::shipbuilding_state::DesignSort::DeltaV,
        super::shipbuilding_state::DesignSort::Combat,
        super::shipbuilding_state::DesignSort::Weight,
    ];

    let mut rows =
        build_design_browser_rows_native(design_library, shipbuilding_data, research_state, ships);
    rows.sort_by(|left, right| compare_design_rows_native(left, right, ui_state.design_sort));
    if ui_state.design_sort_descending {
        rows.reverse();
    }

    let selected_row = ui_state
        .selected_template_id
        .and_then(|template_id| rows.iter().find(|row| row.template_id == template_id));
    let selected_site = ui_state
        .selected_colony
        .and_then(|entity| colonies.get(entity).ok());
    let selected_site_entity = selected_site.map(|(entity, _, _, _)| entity);
    let selected_site_name = selected_site
        .map(|(_, colony, _, _)| colony.name.clone())
        .unwrap_or_else(|| "No retrofit site selected".to_string());
    let selected_retrofit_candidates = selected_row
        .map(|row| {
            retrofit_candidate_count_native(
                row.template_id,
                selected_site_entity,
                design_library,
                ships,
                refits,
            )
        })
        .unwrap_or(0);
    let total_retrofit_candidates = selected_row
        .map(|row| {
            retrofit_candidate_count_native(row.template_id, None, design_library, ships, refits)
        })
        .unwrap_or(0);
    let selected_refit_count = selected_row
        .map(|row| {
            refits
                .iter()
                .filter(|(_, refit)| refit.new_template_id == row.template_id)
                .count()
        })
        .unwrap_or(0);

    commands.entity(library_root).with_children(|parent| {
        parent.spawn(text_block(
            "Design Archive".to_string(),
            14.0,
            Color::srgb(0.55, 0.95, 1.0),
        ));
        parent.spawn(text_block(
            format!(
                "Stored designs: {} | Sort: {}{}",
                rows.len(),
                ui_state.design_sort.label(),
                if ui_state.design_sort_descending { " desc" } else { " asc" }
            ),
            11.0,
            Color::srgb(0.82, 0.87, 0.9),
        ));

        if rows.is_empty() {
            parent.spawn(text_block(
                "No saved designs yet. Save the current draft from the Design tab to populate the archive.".to_string(),
                11.0,
                Color::srgb(0.6, 0.7, 0.76),
            ));
            parent.spawn((
                Button,
                ShipbuildingTabSwitchButton {
                    target: ShipbuildingTab::Design,
                },
                Node {
                    width: Val::Percent(100.0),
                    min_height: Val::Px(34.0),
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.08, 0.2, 0.24)),
                BorderColor::all(Color::srgb(0.2, 0.92, 0.98)),
                Text::new("Open Ship Designer"),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.92, 0.96, 0.98)),
            ));
            return;
        }

        for row in &rows {
            let selected = ui_state.selected_template_id == Some(row.template_id);
            parent.spawn((
                Button,
                ShipbuildingArchiveSelectButton {
                    template_id: row.template_id,
                },
                Node {
                    width: Val::Percent(100.0),
                    min_height: Val::Px(52.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(if selected {
                    Color::srgb(0.1, 0.18, 0.24)
                } else {
                    Color::srgb(0.05, 0.08, 0.12)
                }),
                BorderColor::all(if selected {
                    Color::srgb(0.0, 0.95, 1.0)
                } else {
                    Color::srgb(0.22, 0.35, 0.42)
                }),
                Text::new(format!(
                    "{} v{}\n{} | {} | Active {} | {:.0} m/s",
                    row.name,
                    row.version,
                    row.hull_name,
                    row.construction_mode.display_name(),
                    row.active_ship_count,
                    row.summary.delta_v_ms,
                )),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(Color::srgb(0.88, 0.93, 0.96)),
            ));
        }
    });

    commands.entity(blueprint_root).with_children(|parent| {
        parent.spawn(text_block(
            "Archive Detail".to_string(),
            15.0,
            Color::srgb(0.55, 0.95, 1.0),
        ));

        let Some(row) = selected_row else {
            parent.spawn(text_block(
                "Select an archive entry to inspect it, upgrade it, or queue retrofits for older ships.".to_string(),
                12.0,
                Color::srgb(0.82, 0.87, 0.9),
            ));
            return;
        };

        parent.spawn(text_block(
            format!(
                "{} v{}\nHull: {}\nClass: {:?}\nMass: {}\nBuild: {:.0} BP\nActive ships: {}\nRetrofit candidates: {} total | {} at {}\nRefits in progress: {}",
                row.name,
                row.version,
                row.hull_name,
                row.hull_class,
                format_mass_compact_tonnes(row.summary.launch_mass_t),
                row.summary.build_points,
                row.active_ship_count,
                total_retrofit_candidates,
                selected_retrofit_candidates,
                selected_site_name,
                selected_refit_count,
            ),
            12.0,
            Color::srgb(0.84, 0.9, 0.94),
        ));

        parent.spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(8.0),
                ..default()
            },
        )).with_children(|row_buttons| {
            spawn_archive_action_button(
                row_buttons,
                "Open In Designer",
                row.template_id,
                ShipbuildingArchiveAction::Open,
                Color::srgb(0.0, 0.95, 1.0),
            );
            spawn_archive_action_button(
                row_buttons,
                "Upgrade Design",
                row.template_id,
                ShipbuildingArchiveAction::Upgrade,
                Color::srgb(0.86, 0.78, 0.34),
            );
            spawn_archive_action_button(
                row_buttons,
                "Queue Retrofits",
                row.template_id,
                ShipbuildingArchiveAction::QueueRetrofits,
                Color::srgb(0.38, 0.94, 0.7),
            );
            spawn_archive_action_button(
                row_buttons,
                "Delete",
                row.template_id,
                ShipbuildingArchiveAction::Delete,
                Color::srgb(1.0, 0.42, 0.38),
            );
        });

        if !row.summary.resource_costs.is_empty() {
            parent.spawn(text_block(
                format!(
                    "Material Cost\n{}",
                    format_shipbuilding_resource_cost_lines(&row.summary.resource_costs, 6).join("\n")
                ),
                10.0,
                Color::srgb(0.82, 0.87, 0.9),
            ));
        }
    });

    commands.entity(analytics_root).with_children(|parent| {
        parent.spawn(text_block(
            "Archive Metrics".to_string(),
            14.0,
            Color::srgb(0.55, 0.95, 1.0),
        ));

        parent.spawn(text_block(
            format!("Retrofit Site: {}", selected_site_name),
            11.0,
            Color::srgb(0.82, 0.87, 0.9),
        ));

        let mut colony_rows: Vec<_> = colonies
            .iter()
            .filter(|(_, colony, _, _)| colony.building_count(BuildingType::Shipyard) > 0)
            .collect();
        colony_rows.sort_by(|left, right| left.1.name.cmp(&right.1.name));

        for (entity, colony, _, _) in colony_rows {
            let selected = ui_state.selected_colony == Some(entity);
            parent.spawn((
                Button,
                ShipbuildingConstructionSiteButton { site: entity },
                Node {
                    width: Val::Percent(100.0),
                    min_height: Val::Px(32.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(if selected {
                    Color::srgb(0.1, 0.18, 0.24)
                } else {
                    Color::srgb(0.05, 0.08, 0.12)
                }),
                BorderColor::all(if selected {
                    Color::srgb(0.0, 0.95, 1.0)
                } else {
                    Color::srgb(0.22, 0.35, 0.42)
                }),
                Text::new(format!(
                    "{} | {} shipyards",
                    colony.name,
                    colony.building_count(BuildingType::Shipyard)
                )),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(Color::srgb(0.88, 0.93, 0.96)),
            ));
        }

        if let Some(row) = selected_row {
            spawn_analytics_gauge(
                parent,
                "DV",
                "Delta-V",
                row.summary.delta_v_ms,
                row.summary.delta_v_ms,
                gauge_capacity(row.summary.delta_v_ms, row.summary.delta_v_ms, 100.0),
                "m/s",
                Color::srgb(0.0, 0.95, 1.0),
            );
            spawn_analytics_gauge(
                parent,
                "CV",
                "Combat Value",
                combat_score_native(&row.summary),
                combat_score_native(&row.summary),
                gauge_capacity(
                    combat_score_native(&row.summary),
                    combat_score_native(&row.summary),
                    10.0,
                ),
                "",
                Color::srgb(1.0, 0.6, 0.32),
            );
            spawn_analytics_gauge(
                parent,
                "SNS",
                "Sensor Range",
                row.summary.sensor_range_au,
                row.summary.sensor_range_au,
                gauge_capacity(
                    row.summary.sensor_range_au,
                    row.summary.sensor_range_au,
                    0.1,
                ),
                "AU",
                Color::srgb(0.5, 0.92, 0.9),
            );
            spawn_analytics_chip_row(
                parent,
                &[
                    (
                        "MASS",
                        format_mass_compact_tonnes(row.summary.launch_mass_t),
                        String::new(),
                        Color::srgb(0.5, 0.86, 1.0),
                    ),
                    (
                        "CREW",
                        format!("{:.0}", row.summary.crew),
                        String::new(),
                        Color::srgb(0.86, 0.82, 0.58),
                    ),
                    (
                        "MODE",
                        row.construction_mode.display_name().to_string(),
                        String::new(),
                        Color::srgb(0.68, 0.9, 0.76),
                    ),
                    (
                        "ACTIVE",
                        row.active_ship_count.to_string(),
                        String::new(),
                        Color::srgb(0.5, 0.92, 0.58),
                    ),
                    (
                        "RETRO",
                        selected_retrofit_candidates.to_string(),
                        format!("{} total", total_retrofit_candidates),
                        Color::srgb(0.86, 0.78, 0.34),
                    ),
                ],
            );
        } else {
            parent.spawn(text_block(
                "Archive metrics appear here once an entry is selected.".to_string(),
                11.0,
                Color::srgb(0.6, 0.7, 0.76),
            ));
        }
    });
}

fn populate_construction_tab_native(
    commands: &mut Commands,
    library_root: Entity,
    blueprint_root: Entity,
    analytics_root: Entity,
    colonies: &WorkspaceColonyQuery,
    fleets: &WorkspaceFleetQuery,
    ships: &WorkspaceShipQuery,
    projects: &Query<(Entity, &ShipConstructionProject)>,
    design_library: &ShipDesignLibrary,
    shipbuilding_data: &ShipbuildingData,
    research_state: &ResearchState,
    launch_state: &LaunchCapacityState,
    budget: &GlobalBudget,
    ui_state: &ShipbuildingUiState,
) {
    let design_rows =
        build_design_browser_rows_native(design_library, shipbuilding_data, research_state, ships);
    let selected_colony = ui_state
        .selected_colony
        .and_then(|entity| colonies.get(entity).ok());
    let fleet_rows =
        build_workspace_fleet_rows(selected_colony.map(|(entity, _, _, _)| entity), fleets);
    let selected_design = ui_state
        .construction_design_id
        .and_then(|template_id| design_library.get_template(&template_id));
    let selected_summary = selected_design.and_then(|template| {
        shipbuilding_data.summarize_design(&design_from_template_native(template), research_state)
    });
    let queue_errors = selected_design
        .map(|template| {
            crate::shipbuilding::systems::queue_validation_errors(
                selected_colony.map(|(_, colony, _, _)| colony),
                shipbuilding_data.get_hull(&template.hull_id),
                selected_summary.as_ref(),
                template.construction_mode,
            )
        })
        .unwrap_or_else(|| vec!["Select a saved design before queueing.".to_string()]);

    commands.entity(library_root).with_children(|parent| {
        parent.spawn(text_block(
            "Build Site".to_string(),
            14.0,
            Color::srgb(0.55, 0.95, 1.0),
        ));

        let mut colony_rows: Vec<_> = colonies
            .iter()
            .filter(|(_, colony, _, _)| colony.building_count(BuildingType::Shipyard) > 0)
            .collect();
        colony_rows.sort_by(|left, right| left.1.name.cmp(&right.1.name));

        if colony_rows.is_empty() {
            parent.spawn(text_block(
                "No operational shipyards found.".to_string(),
                11.0,
                Color::srgb(1.0, 0.55, 0.45),
            ));
        }

        for (entity, colony, _, _) in colony_rows {
            let selected = ui_state.selected_colony == Some(entity);
            parent.spawn((
                Button,
                ShipbuildingConstructionSiteButton { site: entity },
                Node {
                    width: Val::Percent(100.0),
                    min_height: Val::Px(44.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(if selected {
                    Color::srgb(0.1, 0.18, 0.24)
                } else {
                    Color::srgb(0.05, 0.08, 0.12)
                }),
                BorderColor::all(if selected {
                    Color::srgb(0.0, 0.95, 1.0)
                } else {
                    Color::srgb(0.22, 0.35, 0.42)
                }),
                Text::new(format!(
                    "{}\n{} shipyards | {:.0} t/yr launch",
                    colony.name,
                    colony.building_count(BuildingType::Shipyard),
                    crate::shipbuilding::systems::annual_launch_capacity_t(colony),
                )),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(Color::srgb(0.88, 0.93, 0.96)),
            ));
        }

        parent.spawn(text_block(
            "Fleet Routing".to_string(),
            13.0,
            Color::srgb(0.55, 0.95, 1.0),
        ));
        parent.spawn((
            Button,
            ShipbuildingConstructionFleetButton { fleet: None },
            Node {
                width: Val::Percent(100.0),
                min_height: Val::Px(32.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(if ui_state.construction_target_fleet.is_none() {
                Color::srgb(0.1, 0.18, 0.24)
            } else {
                Color::srgb(0.05, 0.08, 0.12)
            }),
            BorderColor::all(if ui_state.construction_target_fleet.is_none() {
                Color::srgb(0.0, 0.95, 1.0)
            } else {
                Color::srgb(0.22, 0.35, 0.42)
            }),
            Text::new("Standalone ship pool"),
            TextFont {
                font_size: 10.5,
                ..default()
            },
            TextColor(Color::srgb(0.88, 0.93, 0.96)),
        ));

        for fleet in &fleet_rows {
            let selected = ui_state.construction_target_fleet == Some(fleet.entity);
            parent.spawn((
                Button,
                ShipbuildingConstructionFleetButton {
                    fleet: Some(fleet.entity),
                },
                Node {
                    width: Val::Percent(100.0),
                    min_height: Val::Px(38.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(if selected {
                    Color::srgb(0.1, 0.18, 0.24)
                } else {
                    Color::srgb(0.05, 0.08, 0.12)
                }),
                BorderColor::all(if selected {
                    Color::srgb(0.0, 0.95, 1.0)
                } else {
                    Color::srgb(0.22, 0.35, 0.42)
                }),
                Text::new(format!(
                    "{}\n{:.4} AU | {} ships{}",
                    fleet.name,
                    fleet.orbit_radius_au,
                    fleet.ship_count,
                    if fleet.stationary { " | station" } else { "" },
                )),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(Color::srgb(0.88, 0.93, 0.96)),
            ));
        }
    });

    commands.entity(blueprint_root).with_children(|parent| {
        parent.spawn(text_block(
            "Queue Control".to_string(),
            15.0,
            Color::srgb(0.55, 0.95, 1.0),
        ));

        for row in &design_rows {
            let selected = ui_state.construction_design_id == Some(row.template_id);
            parent.spawn((
                Button,
                ShipbuildingConstructionDesignButton {
                    template_id: row.template_id,
                },
                Node {
                    width: Val::Percent(100.0),
                    min_height: Val::Px(44.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(if selected {
                    Color::srgb(0.1, 0.18, 0.24)
                } else {
                    Color::srgb(0.05, 0.08, 0.12)
                }),
                BorderColor::all(if selected {
                    Color::srgb(0.0, 0.95, 1.0)
                } else {
                    Color::srgb(0.22, 0.35, 0.42)
                }),
                Text::new(format!(
                    "{} v{}\n{} | {} | {:.0} BP",
                    row.name,
                    row.version,
                    row.hull_name,
                    row.construction_mode.display_name(),
                    row.summary.build_points,
                )),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(Color::srgb(0.88, 0.93, 0.96)),
            ));
        }

        if let Some(template) = selected_design {
            parent.spawn(text_block(
                format!(
                    "Selected: {} v{}\nMode: {}\nMass: {}\nDelta-V: {:.0} m/s",
                    template.name,
                    template.version,
                    template.construction_mode.display_name(),
                    selected_summary
                        .as_ref()
                        .map(|summary| format_mass_compact_tonnes(summary.launch_mass_t))
                        .unwrap_or_else(|| "Unknown".to_string()),
                    selected_summary
                        .as_ref()
                        .map(|summary| summary.delta_v_ms)
                        .unwrap_or_default(),
                ),
                11.5,
                Color::srgb(0.84, 0.9, 0.94),
            ));
        }

        parent.spawn((
            Button,
            ShipbuildingQueueSelectedDesignButton,
            Node {
                width: Val::Percent(100.0),
                min_height: Val::Px(34.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(if queue_errors.is_empty() {
                Color::srgb(0.08, 0.2, 0.14)
            } else {
                Color::srgb(0.14, 0.08, 0.09)
            }),
            BorderColor::all(if queue_errors.is_empty() {
                Color::srgb(0.38, 0.94, 0.7)
            } else {
                Color::srgb(1.0, 0.42, 0.38)
            }),
            Text::new("Queue Selected Design"),
            TextFont {
                font_size: 11.0,
                ..default()
            },
            TextColor(Color::srgb(0.92, 0.96, 0.98)),
        ));

        for error in &queue_errors {
            parent.spawn(text_block(
                error.clone(),
                10.0,
                Color::srgb(1.0, 0.55, 0.45),
            ));
        }

        parent.spawn(text_block(
            "If the selected fleet leaves the build site before launch or orbital completion, the ship falls back to an independent hull automatically.".to_string(),
            9.8,
            Color::srgb(0.6, 0.7, 0.76),
        ));
    });

    commands.entity(analytics_root).with_children(|parent| {
        parent.spawn(text_block(
            format!(
                "Shipyard Facilities | Treasury {}",
                crate::economy::format_currency(budget.treasury)
            ),
            14.0,
            Color::srgb(0.55, 0.95, 1.0),
        ));

        if projects.is_empty() {
            parent.spawn(text_block(
                "No ships are under construction. Save a design and queue it to begin building."
                    .to_string(),
                11.0,
                Color::srgb(0.6, 0.7, 0.76),
            ));
            parent.spawn((
                Button,
                ShipbuildingTabSwitchButton {
                    target: ShipbuildingTab::Design,
                },
                Node {
                    width: Val::Percent(100.0),
                    min_height: Val::Px(34.0),
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.08, 0.2, 0.24)),
                BorderColor::all(Color::srgb(0.2, 0.92, 0.98)),
                Text::new("Open Ship Designer"),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.92, 0.96, 0.98)),
            ));
        }

        let mut colony_rows: Vec<_> = colonies
            .iter()
            .filter(|(_, colony, _, _)| colony.building_count(BuildingType::Shipyard) > 0)
            .collect();
        colony_rows.sort_by(|left, right| left.1.name.cmp(&right.1.name));

        for (entity, colony, _, stockpile) in colony_rows {
            let shipyard_count = colony.building_count(BuildingType::Shipyard) as f64;
            let available_launch = launch_state
                .available_mass_t
                .get(&entity)
                .copied()
                .unwrap_or_else(|| crate::shipbuilding::systems::annual_launch_capacity_t(colony));
            let max_launch = crate::shipbuilding::systems::annual_launch_capacity_t(colony);
            parent.spawn(text_block(
                format!(
                    "{}\n{} shipyards | {:.0} / {:.0} t launch | selected {}",
                    colony.name,
                    colony.building_count(BuildingType::Shipyard),
                    available_launch,
                    max_launch,
                    if ui_state.selected_colony == Some(entity) {
                        "yes"
                    } else {
                        "no"
                    },
                ),
                11.0,
                Color::srgb(0.84, 0.9, 0.94),
            ));

            if let Some(stockpile) = stockpile {
                parent.spawn(text_block(
                    format!(
                        "Stockpile Fe {:.2} Mt | Al {:.2} Mt | Polymers {:.2} Mt",
                        stockpile.get(&crate::economy::ResourceType::Iron),
                        stockpile.get(&crate::economy::ResourceType::Aluminum),
                        stockpile.get(&crate::economy::ResourceType::Polymers),
                    ),
                    9.8,
                    Color::srgb(0.6, 0.7, 0.76),
                ));
            }

            for (_, project) in projects
                .iter()
                .filter(|(_, project)| project.build_site == entity)
            {
                parent.spawn(text_block(
                    format!(
                        "  {} | {} | {:.0}% | {}",
                        project.design_name,
                        project.construction_mode.display_name(),
                        project.progress_percent() * 100.0,
                        if project.awaiting_resources {
                            "Awaiting Resources".to_string()
                        } else {
                            project.state.label().to_string()
                        }
                    ),
                    9.8,
                    Color::srgb(0.82, 0.87, 0.9),
                ));
            }

            if shipyard_count > 0.0 {
                parent.spawn((Node {
                    height: Val::Px(4.0),
                    ..default()
                },));
            }
        }
    });
}

fn populate_components_tab_native(
    commands: &mut Commands,
    library_root: Entity,
    blueprint_root: Entity,
    analytics_root: Entity,
    shipbuilding_data: &ShipbuildingData,
    technologies_data: &TechnologiesData,
    research_state: &ResearchState,
    engineering_projects: &Query<&EngineeringProject>,
    ui_state: &ShipbuildingUiState,
) {
    let mut modules: Vec<_> = shipbuilding_data
        .modules
        .values()
        .filter(|module| {
            technologies_data
                .get_component(module.engineering_project_id())
                .is_some_and(|component| {
                    component.required_tech.is_empty()
                        || research_state.is_unlocked(&component.required_tech)
                })
        })
        .collect();
    modules.sort_by(|left, right| left.display_name.cmp(&right.display_name));
    let selected_module = ui_state
        .selected_component_module_id
        .as_deref()
        .and_then(|module_id| shipbuilding_data.get_module(module_id))
        .filter(|module| modules.iter().any(|visible| visible.id == module.id))
        .or_else(|| modules.first().copied());

    commands.entity(library_root).with_children(|parent| {
        if ui_state.selected_component_module_id.is_none() {
            parent.spawn(text_block(
                "Click a module in the list below to inspect its components.".to_string(),
                11.0,
                Color::srgb(0.6, 0.7, 0.76),
            ));
        }

        parent.spawn(text_block(
            "Component Database".to_string(),
            14.0,
            Color::srgb(0.55, 0.95, 1.0),
        ));
        parent.spawn(text_block(
            format!("Available projects: {}", modules.len()),
            11.0,
            Color::srgb(0.82, 0.87, 0.9),
        ));

        parent
            .spawn((Node {
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                overflow: Overflow::scroll_y(),
                ..default()
            },))
            .with_children(|list| {
                for module in modules {
                    let selected = selected_module
                        .is_some_and(|selected_module| selected_module.id == module.id);
                    let status = engineering_status_native(
                        Some(module.engineering_project_id()),
                        technologies_data,
                        research_state,
                        engineering_projects,
                    );
                    let engineering_complete =
                        research_state.is_component_completed(module.engineering_project_id());
                    let row_background = if selected {
                        if engineering_complete {
                            Color::srgb(0.1, 0.18, 0.24)
                        } else {
                            Color::srgb(0.14, 0.14, 0.16)
                        }
                    } else if engineering_complete {
                        Color::srgb(0.05, 0.08, 0.12)
                    } else {
                        Color::srgb(0.08, 0.08, 0.09)
                    };
                    let row_border = if selected {
                        Color::srgb(0.0, 0.95, 1.0)
                    } else if engineering_complete {
                        Color::srgb(0.22, 0.35, 0.42)
                    } else {
                        Color::srgb(0.28, 0.28, 0.3)
                    };
                    let row_text = if selected {
                        Color::srgb(0.88, 0.93, 0.96)
                    } else if engineering_complete {
                        status.color
                    } else {
                        Color::srgb(0.56, 0.58, 0.62)
                    };
                    list.spawn((
                        Button,
                        ShipbuildingComponentDatabaseButton {
                            module_id: module.id.clone(),
                        },
                        Node {
                            width: Val::Percent(100.0),
                            min_height: Val::Px(52.0),
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(row_background),
                        BorderColor::all(row_border),
                        Text::new(format!(
                            "{}\n{} | {} | {:.0} BP | {}",
                            module.display_name,
                            module.category.display_name(),
                            module.size,
                            module.build_points,
                            status.label,
                        )),
                        TextFont {
                            font_size: 9.8,
                            ..default()
                        },
                        TextColor(row_text),
                    ));
                }
            });
    });

    commands.entity(blueprint_root).with_children(|parent| {
        parent.spawn(text_block(
            "Component Detail".to_string(),
            15.0,
            Color::srgb(0.55, 0.95, 1.0),
        ));

        let Some(module) = selected_module else {
            parent.spawn(text_block(
                "No engineering projects available yet. Research new technologies to unlock ship components.".to_string(),
                11.0,
                Color::srgb(0.6, 0.7, 0.76),
            ));
            return;
        };

        let component_name = module
            .engineering_project_id();
        let component = technologies_data.get_component(component_name);
        let tech_name = component
            .and_then(|component| technologies_data.get_tech(&component.required_tech))
            .map(|tech| tech.name.clone())
            .or_else(|| {
                module
                    .required_tech
                    .as_deref()
                    .and_then(|tech_id| technologies_data.get_tech(tech_id))
                    .map(|tech| tech.name.clone())
            })
            .unwrap_or_else(|| "Baseline project".to_string());
        let component_name = component
            .map(|component| component.name.clone())
            .unwrap_or_else(|| module.display_name.clone());
        let status = engineering_status_native(
            Some(module.engineering_project_id()),
            technologies_data,
            research_state,
            engineering_projects,
        );

        parent.spawn(text_block(module.display_name.clone(), 14.0, Color::srgb(0.84, 0.9, 0.94)));
        parent.spawn(text_block(module.description.clone(), 10.5, Color::srgb(0.6, 0.7, 0.76)));
        parent.spawn(text_block(
            format!(
                "Category: {}\nSize: {}\nBuild: {:.0} BP\nMass: {}\nPower: {}",
                module.category.display_name(),
                module.size,
                module.build_points,
                format_mass_compact_tonnes(module.dry_mass_t),
                format_power_profile_native(module),
            ),
            11.0,
            Color::srgb(0.82, 0.87, 0.9),
        ));
        parent.spawn(text_block(
            format!(
                "Technology: {}\nEngineering: {}\nStatus: {}",
                tech_name,
                component_name,
                status.label,
            ),
            11.0,
            status.color,
        ));
        parent.spawn((
            Button,
            ShipbuildingOpenEngineeringButton,
            Node {
                width: Val::Px(220.0),
                min_height: Val::Px(40.0),
                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.06, 0.2, 0.24)),
            BorderColor::all(Color::srgb(0.2, 0.92, 0.98)),
            Text::new(match status.label {
                "Engineering complete. Module can be installed now." => "Engineering Complete",
                "Engineering in progress." => "Open Engineering Queue",
                _ => "Open Component Engineering",
            }),
            TextFont {
                font_size: 11.5,
                ..default()
            },
            TextColor(Color::srgb(0.92, 0.96, 0.98)),
        ));
    });

    commands.entity(analytics_root).with_children(|parent| {
        parent.spawn(text_block(
            "Component Analytics".to_string(),
            14.0,
            Color::srgb(0.55, 0.95, 1.0),
        ));

        if let Some(module) = selected_module {
            spawn_analytics_gauge(
                parent,
                "BP",
                "Build Points",
                module.build_points,
                module.build_points,
                gauge_capacity(module.build_points, module.build_points, 10.0),
                "BP",
                Color::srgb(0.5, 0.92, 0.58),
            );
            spawn_analytics_gauge(
                parent,
                "MASS",
                "Dry Mass",
                module.dry_mass_t,
                module.dry_mass_t,
                gauge_capacity(module.dry_mass_t, module.dry_mass_t, 1.0),
                "t",
                Color::srgb(0.5, 0.86, 1.0),
            );
            spawn_analytics_gauge(
                parent,
                "NET",
                "Net Power",
                module.power_generation_mw - module.power_draw_mw,
                module.power_generation_mw - module.power_draw_mw,
                gauge_capacity(
                    module.power_generation_mw - module.power_draw_mw,
                    module.power_generation_mw - module.power_draw_mw,
                    1.0,
                ),
                "MW",
                Color::srgb(0.0, 0.95, 1.0),
            );
            spawn_analytics_chip_row(
                parent,
                &[
                    (
                        "THR",
                        format!("{:.0} kN", module.thrust_kn),
                        String::new(),
                        Color::srgb(1.0, 0.6, 0.32),
                    ),
                    (
                        "BUILD",
                        format!("{:.0} BP/yr", module.construction_capacity_bp_per_year),
                        String::new(),
                        Color::srgb(0.5, 0.92, 0.9),
                    ),
                    (
                        "LAUNCH",
                        format!("{:.0} t/yr", module.launch_capacity_t_per_year),
                        String::new(),
                        Color::srgb(0.86, 0.82, 0.58),
                    ),
                ],
            );

            if !module.resource_costs.is_empty() {
                parent.spawn(text_block(
                    format!(
                        "Material Cost\n{}",
                        format_shipbuilding_resource_cost_lines(&module.resource_costs, 6)
                            .join("\n")
                    ),
                    10.0,
                    Color::srgb(0.82, 0.87, 0.9),
                ));
            }
        }
    });
}

fn spawn_archive_action_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    template_id: uuid::Uuid,
    action: ShipbuildingArchiveAction,
    accent: Color,
) {
    parent.spawn((
        Button,
        ShipbuildingArchiveActionButton {
            template_id,
            action,
        },
        Node {
            flex_grow: 1.0,
            min_height: Val::Px(32.0),
            padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(accent.with_alpha(0.14)),
        BorderColor::all(accent),
        Text::new(label),
        TextFont {
            font_size: 10.0,
            ..default()
        },
        TextColor(Color::srgb(0.92, 0.96, 0.98)),
    ));
}

fn build_workspace_fleet_rows(
    selected_colony: Option<Entity>,
    fleets: &WorkspaceFleetQuery,
) -> Vec<WorkspaceFleetRow> {
    let Some(build_site) = selected_colony else {
        return Vec::new();
    };

    let mut rows: Vec<_> = fleets
        .iter()
        .filter_map(|(fleet_entity, fleet, orbit, maneuver)| {
            if maneuver.is_some() || orbit.body != build_site {
                return None;
            }

            Some(WorkspaceFleetRow {
                entity: fleet_entity,
                name: fleet.name.clone(),
                orbit_radius_au: orbit.radius_au,
                stationary: orbit.direction == 0.0,
                ship_count: fleet.ships.len(),
            })
        })
        .collect();
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    rows
}

fn template_descends_from_native(
    design_library: &ShipDesignLibrary,
    candidate_id: uuid::Uuid,
    ancestor_id: uuid::Uuid,
) -> bool {
    let mut cursor = Some(candidate_id);
    while let Some(template_id) = cursor {
        if template_id == ancestor_id {
            return true;
        }

        cursor = design_library
            .get_template(&template_id)
            .and_then(|template| template.parent_template_id);
    }

    false
}

fn template_has_descendants_native(
    design_library: &ShipDesignLibrary,
    template_id: uuid::Uuid,
) -> bool {
    design_library
        .templates
        .keys()
        .copied()
        .filter(|candidate_id| *candidate_id != template_id)
        .any(|candidate_id| {
            template_descends_from_native(design_library, candidate_id, template_id)
        })
}

fn retrofit_candidate_count_native(
    template_id: uuid::Uuid,
    build_site: Option<Entity>,
    design_library: &ShipDesignLibrary,
    ships: &WorkspaceShipQuery,
    refits: &Query<(Entity, &RefitProject)>,
) -> usize {
    let active_refits: std::collections::HashSet<_> =
        refits.iter().map(|(_, refit)| refit.ship_entity).collect();

    ships
        .iter()
        .filter(|(ship_entity, ship, assignment)| {
            !active_refits.contains(ship_entity)
                && assignment.template_id != template_id
                && build_site.is_none_or(|site| ship.parked_body == site)
                && template_descends_from_native(
                    design_library,
                    template_id,
                    assignment.template_id,
                )
        })
        .count()
}

fn build_design_browser_rows_native(
    design_library: &ShipDesignLibrary,
    shipbuilding_data: &ShipbuildingData,
    research_state: &ResearchState,
    ships: &WorkspaceShipQuery,
) -> Vec<WorkspaceDesignRow> {
    let mut rows = Vec::new();
    let mut active_ship_counts = HashMap::new();
    for (_, _, assignment) in ships.iter() {
        *active_ship_counts
            .entry(assignment.template_id)
            .or_insert(0usize) += 1;
    }

    for template in design_library.all_templates() {
        let draft = design_from_template_native(template);
        if let Some(summary) = shipbuilding_data.summarize_design(&draft, research_state) {
            let hull_name = shipbuilding_data
                .get_hull(&template.hull_id)
                .map(|hull| hull.display_name.clone())
                .unwrap_or_else(|| template.hull_id.clone());
            rows.push(WorkspaceDesignRow {
                template_id: template.id,
                name: template.name.clone(),
                version: template.version,
                hull_name,
                hull_class: summary.ship_class,
                summary,
                construction_mode: template.construction_mode,
                active_ship_count: active_ship_counts.get(&template.id).copied().unwrap_or(0),
            });
        }
    }
    rows
}

fn compare_design_rows_native(
    left: &WorkspaceDesignRow,
    right: &WorkspaceDesignRow,
    sort: super::shipbuilding_state::DesignSort,
) -> std::cmp::Ordering {
    match sort {
        super::shipbuilding_state::DesignSort::HullType => left
            .hull_class
            .display_name()
            .cmp(right.hull_class.display_name())
            .then_with(|| left.name.cmp(&right.name)),
        super::shipbuilding_state::DesignSort::DeltaV => left
            .summary
            .delta_v_ms
            .partial_cmp(&right.summary.delta_v_ms)
            .unwrap_or(std::cmp::Ordering::Equal),
        super::shipbuilding_state::DesignSort::Combat => combat_score_native(&left.summary)
            .partial_cmp(&combat_score_native(&right.summary))
            .unwrap_or(std::cmp::Ordering::Equal),
        super::shipbuilding_state::DesignSort::Weight => left
            .summary
            .launch_mass_t
            .partial_cmp(&right.summary.launch_mass_t)
            .unwrap_or(std::cmp::Ordering::Equal),
    }
}

fn combat_score_native(summary: &ShipDesignSummary) -> f64 {
    summary.ordnance_capacity_t * 12.0
        + summary.magazine_capacity_t * 6.0
        + summary.sensor_range_au * 20.0
        + summary.thrust_kn * 0.01
        + summary.power_generation_mw.max(0.0) * 0.05
}

fn design_from_template_native(
    template: &crate::shipbuilding::ShipDesignTemplate,
) -> ShipDesignDraft {
    ShipDesignDraft {
        name: template.name.clone(),
        hull_id: template.hull_id.clone(),
        modules: template.modules.clone(),
        construction_mode: template.construction_mode,
    }
}

fn save_current_design_template_native(
    design_library: &mut ShipDesignLibrary,
    ui_state: &mut ShipbuildingUiState,
) -> Option<uuid::Uuid> {
    let design = build_preview_design(ui_state)?;
    let name = if design.name.trim().is_empty() {
        "Unnamed Design".to_string()
    } else {
        design.name.trim().to_string()
    };

    let parent_template_id = ui_state.upgrade_source_template_id;
    let version = parent_template_id
        .and_then(|template_id| {
            design_library
                .get_template(&template_id)
                .map(|template| template.version + 1)
        })
        .unwrap_or_else(|| design_library.latest_version(&name) + 1);

    let template_id = uuid::Uuid::new_v4();
    design_library.save_template(crate::shipbuilding::ShipDesignTemplate {
        id: template_id,
        name: name.clone(),
        hull_id: design.hull_id,
        modules: design.modules,
        version,
        parent_template_id,
        created_at_game_time: 0.0,
        construction_mode: design.construction_mode,
    });

    ui_state.selected_template_id = Some(template_id);
    ui_state.construction_design_id = Some(template_id);
    ui_state.upgrade_source_template_id = None;
    Some(template_id)
}

fn load_template_into_ui_native(
    ui_state: &mut ShipbuildingUiState,
    shipbuilding_data: &ShipbuildingData,
    research_state: &ResearchState,
    template: &crate::shipbuilding::ShipDesignTemplate,
) {
    ui_state.selected_template_id = Some(template.id);
    ui_state.selected_hull_id = Some(template.hull_id.clone());
    ui_state.design_name = template.name.clone();
    ui_state.selected_mode = template.construction_mode;
    ui_state.selected_modules = template
        .modules
        .iter()
        .map(|selection| (selection.slot_id.clone(), selection.module_id.clone()))
        .collect();
    ui_state.selected_slot = template
        .modules
        .first()
        .map(|selection| selection.slot_id.clone());
    ui_state.preview_slot = None;
    ui_state.preview_module_id = None;
    hydrate_selected_design_native(ui_state, shipbuilding_data, research_state);
}

fn engineering_status_native(
    component_id: Option<&str>,
    technologies_data: &TechnologiesData,
    research_state: &ResearchState,
    engineering_projects: &Query<&EngineeringProject>,
) -> EngineeringStatusNative {
    let Some(component_id) = component_id else {
        return EngineeringStatusNative {
            label: "Production-ready: no engineering project required.",
            color: Color::srgb(0.6, 0.7, 0.76),
        };
    };

    if research_state.is_component_completed(component_id) {
        return EngineeringStatusNative {
            label: "Engineering complete. Module can be installed now.",
            color: Color::srgb(0.5, 0.92, 0.62),
        };
    }

    if engineering_projects
        .iter()
        .any(|project| project.component_id == component_id)
    {
        return EngineeringStatusNative {
            label: "Engineering in progress.",
            color: Color::srgb(0.98, 0.78, 0.36),
        };
    }

    if let Some(component) = technologies_data.get_component(component_id) {
        if component.required_tech.is_empty()
            || research_state.is_unlocked(&component.required_tech)
        {
            return EngineeringStatusNative {
                label: "Engineering available. Open Research to start the project.",
                color: Color::srgb(0.55, 0.95, 1.0),
            };
        }

        return EngineeringStatusNative {
            label: "Locked by prerequisite technology.",
            color: Color::srgb(1.0, 0.45, 0.4),
        };
    }

    EngineeringStatusNative {
        label: "Engineering definition missing from technology data.",
        color: Color::srgb(1.0, 0.45, 0.4),
    }
}

fn format_power_profile_native(module: &crate::shipbuilding::ShipModuleDefinition) -> String {
    format!(
        "Gen {:+.1} MW | Draw {:.1} MW | Net {:+.1} MW",
        module.power_generation_mw,
        module.power_draw_mw,
        module.power_generation_mw - module.power_draw_mw,
    )
}

fn effective_design_name(ui_state: &ShipbuildingUiState, fallback_hull_name: &str) -> String {
    if ui_state.design_name.trim().is_empty() {
        format!("{} Prototype", fallback_hull_name)
    } else {
        ui_state.design_name.trim().to_string()
    }
}
