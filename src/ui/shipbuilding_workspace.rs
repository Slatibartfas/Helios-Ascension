use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::shipbuilding_state::ShipbuildingUiState;
use crate::game_state::{ActiveMenu, GameMenu};
use crate::research::ResearchState;
use crate::shipbuilding::{
    HullSlotDefinition, ShipDesignDraft, ShipDesignLibrary, ShipDesignSummary, ShipModuleSelection,
    ShipbuildingData,
};
use crate::shipbuilding::types::ShipModuleCategory;

pub(super) struct ShipbuildingWorkspacePlugin;

impl Plugin for ShipbuildingWorkspacePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShipbuildingUiBackend>()
            .add_systems(Startup, spawn_shipbuilding_workspace)
            .add_systems(
                Update,
                (
                    toggle_shipbuilding_ui_backend,
                    handle_shipbuilding_workspace_interactions,
                    animate_shipbuilding_slot_scanlines,
                    animate_shipbuilding_slot_feedback,
                    animate_shipbuilding_module_card_feedback,
                    update_shipbuilding_hover_tooltip,
                    sync_shipbuilding_workspace_visibility,
                    sync_shipbuilding_workspace_content,
                ),
            );
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ShipbuildingUiBackend {
    pub mode: ShipbuildingUiMode,
}

impl ShipbuildingUiBackend {
    pub(crate) fn uses_legacy_egui(&self) -> bool {
        self.mode == ShipbuildingUiMode::LegacyEgui
    }

    pub(crate) fn uses_native_workspace(&self) -> bool {
        self.mode == ShipbuildingUiMode::NativePrototype
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ShipbuildingUiMode {
    #[default]
    LegacyEgui,
    NativePrototype,
}

#[derive(Component)]
struct ShipbuildingWorkspaceRoot;

#[derive(Component)]
struct ShipbuildingWorkspaceStatus;

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
struct ShipbuildingSlotScanline {
    period_scale: f32,
}

#[derive(Component)]
struct ShipbuildingSlotDash;

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
    accent: Color,
}

#[derive(Component)]
struct ShipbuildingSlotOrbitRunner {
    slot_id: String,
    width: f32,
    height: f32,
    phase_offset: f32,
}

#[derive(Component)]
struct ShipbuildingModuleCard {
    slot_id: String,
    module_id: String,
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
                top: Val::Px(72.0),
                bottom: Val::Px(42.0),
                display: Display::None,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(12.0)),
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.02, 0.035, 0.96)),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        min_height: Val::Px(24.0),
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
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

            parent
                .spawn((
                    Name::new("Shipbuilding Workspace Columns"),
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
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
                        width: Val::Px(292.0),
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
                        Text::new(""),
                        TextFont {
                            font_size: 10.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.84, 0.9, 0.94)),
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
        height: Val::Percent(100.0),
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
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    ..default()
                },
            ));
        });
}

fn sync_shipbuilding_workspace_visibility(
    active_menu: Res<ActiveMenu>,
    backend: Res<ShipbuildingUiBackend>,
    mut roots: Query<&mut Node, With<ShipbuildingWorkspaceRoot>>,
) {
    if !active_menu.is_changed() && !backend.is_changed() {
        return;
    }

    let display = if active_menu.current == GameMenu::Shipbuilding && backend.uses_native_workspace()
    {
        Display::Flex
    } else {
        Display::None
    };

    for mut node in &mut roots {
        node.display = display;
    }
}

fn toggle_shipbuilding_ui_backend(
    active_menu: Res<ActiveMenu>,
    input: Res<ButtonInput<KeyCode>>,
    mut backend: ResMut<ShipbuildingUiBackend>,
) {
    if active_menu.current != GameMenu::Shipbuilding || !input.just_pressed(KeyCode::F9) {
        return;
    }

    backend.mode = match backend.mode {
        ShipbuildingUiMode::LegacyEgui => ShipbuildingUiMode::NativePrototype,
        ShipbuildingUiMode::NativePrototype => ShipbuildingUiMode::LegacyEgui,
    };

    info!("Shipbuilding UI backend switched to {:?}", backend.mode);
}

fn handle_shipbuilding_workspace_interactions(
    active_menu: Res<ActiveMenu>,
    backend: Res<ShipbuildingUiBackend>,
    shipbuilding_data: Res<ShipbuildingData>,
    research_state: Res<ResearchState>,
    mut ui_state: ResMut<ShipbuildingUiState>,
    hull_dropdown_toggle: Query<&Interaction, (Changed<Interaction>, With<ShipbuildingHullDropdownToggle>, With<Button>)>,
    hull_option_buttons: Query<(&Interaction, &ShipbuildingHullOptionButton), (Changed<Interaction>, With<Button>)>,
    category_buttons: Query<(&Interaction, &ShipbuildingCategoryButton), (Changed<Interaction>, With<Button>)>,
    clear_buttons: Query<&Interaction, (Changed<Interaction>, With<ShipbuildingClearSlotButton>, With<Button>)>,
    slot_buttons: Query<(&Interaction, &ShipbuildingSlotButton), (Changed<Interaction>, With<Button>)>,
    module_buttons: Query<(&Interaction, &ShipbuildingModuleButton), (Changed<Interaction>, With<Button>)>,
) {
    if active_menu.current != GameMenu::Shipbuilding || !backend.uses_native_workspace() {
        return;
    }

    for interaction in &hull_dropdown_toggle {
        if *interaction == Interaction::Pressed {
            ui_state.show_hull_dropdown = !ui_state.show_hull_dropdown;
        }
    }

    for (interaction, button) in &hull_option_buttons {
        if *interaction == Interaction::Pressed {
            select_hull_by_id(
                &mut ui_state,
                &shipbuilding_data,
                &research_state,
                button.hull_id.as_str(),
            );
            ui_state.show_hull_dropdown = false;
        }
    }

    for (interaction, button) in &category_buttons {
        if *interaction == Interaction::Pressed {
            select_first_slot_in_category(&mut ui_state, &shipbuilding_data, button.category);
        }
    }

    for interaction in &clear_buttons {
        if *interaction == Interaction::Pressed {
            if let Some(slot_id) = ui_state.selected_slot.clone() {
                ui_state.selected_modules.remove(&slot_id);
                ui_state.preview_slot = Some(slot_id);
                ui_state.preview_module_id = None;
            }
        }
    }

    for (interaction, slot_button) in &slot_buttons {
        if *interaction == Interaction::Pressed {
            ui_state.selected_slot = Some(slot_button.slot_id.clone());
            ui_state.preview_slot = None;
            ui_state.preview_module_id = None;
        }
    }

    let mut hovered_preview = None;
    let mut clear_preview = false;
    let mut hovered_slot = None;
    let mut hovered_module = None;

    for (interaction, module_button) in &module_buttons {
        match *interaction {
            Interaction::Pressed => {
                ui_state.selected_slot = Some(module_button.slot_id.clone());
                ui_state.selected_modules.insert(
                    module_button.slot_id.clone(),
                    module_button.module_id.clone(),
                );
                ui_state.preview_slot = Some(module_button.slot_id.clone());
                ui_state.preview_module_id = Some(module_button.module_id.clone());
            }
            Interaction::Hovered => {
                hovered_preview = Some((
                    module_button.slot_id.clone(),
                    module_button.module_id.clone(),
                ));
                hovered_module = Some(module_button.module_id.clone());
                hovered_slot = Some(module_button.slot_id.clone());
            }
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

    for (interaction, slot_button) in &slot_buttons {
        if *interaction == Interaction::Hovered {
            hovered_slot = Some(slot_button.slot_id.clone());
        }
    }

    if let Some((slot_id, module_id)) = hovered_preview {
        ui_state.preview_slot = Some(slot_id);
        ui_state.preview_module_id = Some(module_id);
    } else if clear_preview {
        ui_state.preview_slot = None;
        ui_state.preview_module_id = None;
    }

    ui_state.hovered_slot = hovered_slot;
    ui_state.hovered_module_id = hovered_module;
}

fn sync_shipbuilding_workspace_content(
    mut commands: Commands,
    active_menu: Res<ActiveMenu>,
    backend: Res<ShipbuildingUiBackend>,
    ui_state: Res<ShipbuildingUiState>,
    shipbuilding_data: Res<ShipbuildingData>,
    design_library: Res<ShipDesignLibrary>,
    research_state: Res<ResearchState>,
    mut status_text: Single<&mut Text, (With<ShipbuildingWorkspaceStatus>, Without<ShipbuildingWorkspaceAnalytics>, Without<ShipbuildingWorkspaceBlueprint>)>,
    library_root: Single<Entity, (With<ShipbuildingWorkspaceLibrary>, Without<ShipbuildingWorkspaceBlueprint>, Without<ShipbuildingWorkspaceAnalytics>)>,
    blueprint_root: Single<Entity, (With<ShipbuildingWorkspaceBlueprint>, Without<ShipbuildingWorkspaceLibrary>, Without<ShipbuildingWorkspaceAnalytics>)>,
    analytics_root: Single<Entity, (With<ShipbuildingWorkspaceAnalytics>, Without<ShipbuildingWorkspaceLibrary>, Without<ShipbuildingWorkspaceBlueprint>)>,
    child_lists: Query<&Children>,
) {
    if active_menu.current != GameMenu::Shipbuilding || !backend.uses_native_workspace() {
        return;
    }

    if !backend.is_changed()
        && !ui_state.is_changed()
        && !shipbuilding_data.is_changed()
        && !design_library.is_changed()
        && !research_state.is_changed()
    {
        return;
    }

    let available_hulls = shipbuilding_data.available_hulls(&research_state);
    let selected_hull = ui_state
        .selected_hull_id
        .as_deref()
        .and_then(|hull_id| shipbuilding_data.get_hull(hull_id));
    let current_design = build_preview_design(&ui_state);
    let current_summary = current_design
        .as_ref()
        .and_then(|design| shipbuilding_data.summarize_design(design, &research_state));
    let preview_summary = build_preview_summary(&ui_state, &shipbuilding_data, &research_state);
    let active_slot = selected_hull.and_then(|hull| active_slot(hull, &ui_state));

    **status_text = Text::new(format!(
        "Shipbuilding Workspace  |  F9 Switch UI  |  {:?}  |  Hulls {}  |  Designs {}  |  Hover inspect  |  Click install",
        backend.mode,
        available_hulls.len(),
        design_library.templates.len()
    ));

    clear_dynamic_children(&mut commands, *library_root, &child_lists);
    clear_dynamic_children(&mut commands, *blueprint_root, &child_lists);
    clear_dynamic_children(&mut commands, *analytics_root, &child_lists);

    populate_library_panel(
        &mut commands,
        *library_root,
        &available_hulls,
        selected_hull,
        active_slot,
        &ui_state,
        &shipbuilding_data,
        &research_state,
    );
    populate_blueprint_panel(
        &mut commands,
        *blueprint_root,
        selected_hull,
        &ui_state,
        &shipbuilding_data,
    );
    populate_analytics_panel(
        &mut commands,
        *analytics_root,
        selected_hull,
        current_summary.as_ref(),
        preview_summary.as_ref(),
        &ui_state,
    );
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
                None => "No hull selected. Choose a hull in the legacy design tab, then switch back with F9 to inspect the native workspace.".to_string(),
            },
            12.0,
            Color::srgb(0.82, 0.87, 0.9),
        ));

        if let Some(slot) = active_slot {
            parent.spawn(text_block(
                format!(
                    "{} | {} slot",
                    ascii_category_tag(slot.category),
                    slot.size
                ),
                13.0,
                category_color(slot.category),
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

            for module in shipbuilding_data.compatible_modules_for_slot(slot, research_state) {
                let installed = ui_state.selected_modules.get(&slot.slot_id) == Some(&module.id);
                let previewed = ui_state.preview_slot.as_deref() == Some(slot.slot_id.as_str())
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
                    },
                    ShipbuildingModuleButton {
                        slot_id: slot.slot_id.clone(),
                        module_id: module.id.clone(),
                    },
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(62.0),
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
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
                        "{}\n{} • {:.0} t • {:.0} BP\nPower {:+.0} MW  Thrust {:.0} kN",
                        module.display_name,
                        module.size,
                        module.dry_mass_t,
                        module.build_points,
                        module.power_generation_mw - module.power_draw_mw,
                        module.thrust_kn,
                    )),
                    TextFont {
                        font_size: 11.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.88, 0.93, 0.96)),
                ));
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

fn update_shipbuilding_hover_tooltip(
    active_menu: Res<ActiveMenu>,
    backend: Res<ShipbuildingUiBackend>,
    ui_state: Res<ShipbuildingUiState>,
    shipbuilding_data: Res<ShipbuildingData>,
    research_state: Res<ResearchState>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    mut tooltip_node: Single<&mut Node, With<ShipbuildingHoverTooltip>>,
    mut tooltip_title: Single<&mut Text, (With<ShipbuildingHoverTooltipTitle>, Without<ShipbuildingHoverTooltipBody>)>,
    mut tooltip_body: Single<&mut Text, (With<ShipbuildingHoverTooltipBody>, Without<ShipbuildingHoverTooltipTitle>)>,
) {
    if active_menu.current != GameMenu::Shipbuilding || !backend.uses_native_workspace() {
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
            tooltip_node.display = Display::Flex;
            tooltip_node.left = Val::Px((cursor.x + 16.0).min(window.width() - 304.0));
            tooltip_node.top = Val::Px((cursor.y + 12.0).min(window.height() - 176.0));
            **tooltip_title = Text::new(module.display_name.clone());
            **tooltip_body = Text::new(format!(
                "{}\n{} slot\nMass {:.1} t\nBuild {:.0} BP\nNet power {:+.0} MW\nThrust {:.0} kN\nFitting: {}\nLegend: {}",
                module.description,
                module.size,
                module.dry_mass_t,
                module.build_points,
                module.power_generation_mw - module.power_draw_mw,
                module.thrust_kn,
                if let Some(slot_id) = ui_state.hovered_slot.as_deref() {
                    prettify_slot_name(slot_id)
                } else {
                    "Current slot".to_string()
                },
                module_indicator_legend(module)
            ));
            return;
        }
    }

    if let Some(slot_id) = ui_state.hovered_slot.as_deref() {
        if let Some(hull_id) = ui_state.selected_hull_id.as_deref() {
            if let Some(hull) = shipbuilding_data.get_hull(hull_id) {
                if let Some(slot) = hull.slot_layout.iter().find(|slot| slot.slot_id == slot_id) {
                    let module_preview = shipbuilding_data
                        .compatible_modules_for_slot(slot, &research_state)
                        .first()
                        .map(|module| module.display_name.as_str())
                        .unwrap_or("No compatible modules");
                    tooltip_node.display = Display::Flex;
                    tooltip_node.left = Val::Px((cursor.x + 16.0).min(window.width() - 304.0));
                    tooltip_node.top = Val::Px((cursor.y + 12.0).min(window.height() - 176.0));
                    **tooltip_title = Text::new(prettify_slot_name(slot_id));
                    **tooltip_body = Text::new(format!(
                        "{}\n{} slot\n{}\nSuggested fit: {}\nLegend: {}",
                        slot.category.display_name(),
                        slot.size,
                        if slot.required { "Required socket" } else { "Optional socket" },
                        module_preview,
                        slot_indicator_legend(slot),
                    ));
                    return;
                }
            }
        }
    }

    tooltip_node.display = Display::None;
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

            let material_lines = summary
                .resource_costs
                .iter()
                .take(6)
                .map(|(resource, amount)| format!("{} {:.1}", resource.display_name(), amount))
                .collect::<Vec<_>>()
                .join("\n");
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

fn animate_shipbuilding_slot_scanlines(
    active_menu: Res<ActiveMenu>,
    backend: Res<ShipbuildingUiBackend>,
    time: Res<Time<Real>>,
    mut scanlines: Query<(&ShipbuildingSlotScanline, &mut Node, &mut BackgroundColor)>,
) {
    if active_menu.current != GameMenu::Shipbuilding || !backend.uses_native_workspace() {
        return;
    }

    let elapsed = time.elapsed_secs();
    for (scanline, mut node, mut color) in &mut scanlines {
        let phase = (elapsed * scanline.period_scale).fract();
        node.top = Val::Percent(8.0 + phase * 72.0);
        color.0.set_alpha(0.06 + (1.0 - (phase - 0.5).abs() * 2.0) * 0.12);
    }
}

fn animate_shipbuilding_slot_feedback(
    active_menu: Res<ActiveMenu>,
    backend: Res<ShipbuildingUiBackend>,
    ui_state: Res<ShipbuildingUiState>,
    time: Res<Time<Real>>,
    mut glows: Query<
        (&ShipbuildingSlotGlow, &mut BackgroundColor),
        (
            Without<ShipbuildingSlotFrame>,
            Without<ShipbuildingSlotOrbitRunner>,
        ),
    >,
    mut frames: Query<
        (&ShipbuildingSlotFrame, &mut BackgroundColor, &mut BorderColor),
        (
            Without<ShipbuildingSlotGlow>,
            Without<ShipbuildingSlotOrbitRunner>,
        ),
    >,
    mut runners: Query<
        (&ShipbuildingSlotOrbitRunner, &mut Node, &mut BackgroundColor),
        (
            Without<ShipbuildingSlotGlow>,
            Without<ShipbuildingSlotFrame>,
        ),
    >,
) {
    if active_menu.current != GameMenu::Shipbuilding || !backend.uses_native_workspace() {
        return;
    }

    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 2.0).sin();

    for (glow, mut background) in &mut glows {
        let is_selected = ui_state.selected_slot.as_deref() == Some(glow.slot_id.as_str());
        let is_hovered = ui_state.hovered_slot.as_deref() == Some(glow.slot_id.as_str());
        let alpha = if is_selected {
            0.14 + pulse * 0.1
        } else if is_hovered {
            0.11
        } else {
            0.02
        };
        background.0 = glow.accent.with_alpha(alpha);
    }

    for (frame, mut background, mut border) in &mut frames {
        let is_selected = ui_state.selected_slot.as_deref() == Some(frame.slot_id.as_str());
        let is_hovered = ui_state.hovered_slot.as_deref() == Some(frame.slot_id.as_str());
        let mut fill = if frame.filled {
            Color::srgb(0.055, 0.16, 0.19)
        } else {
            Color::srgba(0.02, 0.04, 0.065, 0.9)
        };

        if is_selected {
            fill = mix_color(fill, frame.accent, 0.14 + pulse * 0.08);
            *border = BorderColor::all(mix_color(
                Color::srgb(0.55, 0.95, 1.0),
                frame.accent,
                0.35 + pulse * 0.15,
            ));
        } else if is_hovered {
            fill = mix_color(fill, frame.accent, 0.1);
            *border = BorderColor::all(mix_color(
                Color::srgb(0.36, 0.88, 0.98),
                frame.accent,
                0.2,
            ));
        } else if frame.previewed {
            *border = BorderColor::all(Color::srgb(0.46, 0.78, 1.0));
        } else if frame.filled {
            *border = BorderColor::all(Color::srgb(0.34, 0.86, 0.94));
        } else {
            *border = BorderColor::all(Color::srgb(0.22, 0.45, 0.54));
        }

        background.0 = fill;
    }

    for (runner, mut node, mut color) in &mut runners {
        let is_selected = ui_state.selected_slot.as_deref() == Some(runner.slot_id.as_str());
        node.display = if is_selected { Display::Flex } else { Display::None };
        if !is_selected {
            continue;
        }

        let distance = ((time.elapsed_secs() * 48.0) + runner.phase_offset * 96.0)
            % slot_orbit_perimeter(runner.width, runner.height);
        let (left, top, width, height) = orbit_runner_rect(distance, runner.width, runner.height);
        node.left = Val::Px(left);
        node.top = Val::Px(top);
        node.width = Val::Px(width);
        node.height = Val::Px(height);
        color.0 = Color::srgba(0.9, 1.0, 1.0, 0.75 + pulse * 0.2);
    }
}

fn animate_shipbuilding_module_card_feedback(
    active_menu: Res<ActiveMenu>,
    backend: Res<ShipbuildingUiBackend>,
    ui_state: Res<ShipbuildingUiState>,
    time: Res<Time<Real>>,
    mut cards: Query<(&Interaction, &ShipbuildingModuleCard, &mut BackgroundColor, &mut BorderColor), With<Button>>,
) {
    if active_menu.current != GameMenu::Shipbuilding || !backend.uses_native_workspace() {
        return;
    }

    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 2.4).sin();
    for (interaction, card, mut background, mut border) in &mut cards {
        let installed = ui_state.selected_modules.get(&card.slot_id) == Some(&card.module_id);
        let previewed = ui_state.preview_slot.as_deref() == Some(card.slot_id.as_str())
            && ui_state.preview_module_id.as_deref() == Some(card.module_id.as_str());

        background.0 = if installed {
            mix_color(Color::srgb(0.1, 0.32, 0.22), Color::srgb(0.38, 0.94, 0.7), 0.08 + pulse * 0.06)
        } else if *interaction == Interaction::Hovered {
            Color::srgb(0.09, 0.14, 0.2)
        } else if previewed {
            Color::srgb(0.12, 0.22, 0.34)
        } else {
            Color::srgb(0.055, 0.08, 0.12)
        };

        *border = BorderColor::all(if installed {
            Color::srgb(0.38, 0.94, 0.7)
        } else if *interaction == Interaction::Hovered {
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

    if let Some(existing) = design.modules.iter_mut().find(|selection| selection.slot_id == slot_id) {
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
    if slot_id.contains("tank") || slot_id.contains("cargo") || slot_id.contains("fuel") || slot_id.contains("storage") {
        return 1;
    }
    if slot_id.contains("reactor") || slot_id.contains("power") || slot_id.contains("battery") || slot_id.contains("heat") || slot_id.contains("radiator") {
        return 2;
    }
    if slot_id.contains("sensor") || slot_id.contains("mission") || slot_id.contains("utility") || slot_id.contains("aux") || slot_id.contains("support") {
        return 3;
    }
    if slot_id.contains("hangar") || slot_id.contains("bay") || slot_id.contains("isru") || slot_id.contains("mining") || slot_id.contains("construction") {
        return 4;
    }
    if slot_id.contains("command") || slot_id.contains("bridge") || slot_id.contains("cic") || slot_id.contains("crew") {
        return 5;
    }
    if slot_id.contains("armor") || slot_id.contains("magazine") || slot_id.contains("pd") || slot_id.contains("shield") || slot_id.contains("defense") {
        return 6;
    }
    if slot_id.contains("weapon") || slot_id.contains("spinal") || slot_id.contains("missile") || slot_id.contains("gun") || slot_id.contains("laser") {
        return 7;
    }

    match slot.category {
        ShipModuleCategory::FlightSystems => 0,
        ShipModuleCategory::FuelStorage => 1,
        ShipModuleCategory::PowerThermal => 2,
        ShipModuleCategory::Sensors
        | ShipModuleCategory::UtilitySupport
        | ShipModuleCategory::SpecialScience
        | ShipModuleCategory::ElectronicWarfare => 3,
        ShipModuleCategory::ConstructionISRU => 4,
        ShipModuleCategory::CrewSystems => 5,
        ShipModuleCategory::ArmorDefense => 6,
        ShipModuleCategory::Weapons | ShipModuleCategory::FireControl => 7,
    }
}

fn slot_dimensions(size: &str) -> (f32, f32) {
    let _ = size;
    (132.0, 60.0)
}

fn spawn_blueprint_guides(parent: &mut ChildSpawnerCommands) {
    let sections = [
        (8.0, "Engines"),
        (18.0, "Fuel"),
        (29.0, "Power"),
        (41.0, "Support"),
        (54.0, "Industry"),
        (67.0, "Command"),
        (80.0, "Defense"),
        (90.0, "Weapons"),
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
    let row_stride = 10.5;
    let base_row = 13.0 + row as f32 * row_stride;
    let slot_id = slot.slot_id.to_ascii_lowercase();

    let directional_bias = if slot_id.contains("port") || slot_id.contains("left") {
        -5.0
    } else if slot_id.contains("starboard") || slot_id.contains("right") {
        5.0
    } else if slot_id.contains("front") || slot_id.contains("bow") {
        -3.0
    } else if slot_id.contains("aft") || slot_id.contains("rear") {
        3.0
    } else {
        0.0
    };

    (base_row + directional_bias).clamp(10.0, 82.0)
}

fn zone_bounds(zone: usize) -> (f32, f32) {
    match zone {
        0 => (6.0, 15.0),
        1 => (16.0, 26.0),
        2 => (27.0, 38.0),
        3 => (39.0, 51.0),
        4 => (52.0, 64.0),
        5 => (65.0, 77.0),
        6 => (78.0, 88.0),
        _ => (89.0, 97.0),
    }
}

fn zone_columns(zone: usize, total: u32) -> u32 {
    let _ = (zone, total);
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
    let accent = slot_accent_color(slot.category);
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
                accent,
            },
            BackgroundColor(accent.with_alpha(if is_selected { 0.16 } else if is_hovered { 0.1 } else { 0.02 })),
        ))
        .with_children(|slot_root| {
            slot_root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(3.0),
                    top: Val::Px(0.0),
                    width: Val::Px(4.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(accent),
            ));

            if filled {
                slot_root.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    ShipbuildingSlotFrame {
                        slot_id: slot.slot_id.clone(),
                        accent,
                        filled,
                        previewed: is_previewed,
                    },
                    BackgroundColor(Color::srgb(0.055, 0.16, 0.19)),
                    BorderColor::all(if is_selected {
                        Color::srgb(0.0, 0.98, 1.0)
                    } else if is_hovered {
                        Color::srgb(0.36, 0.88, 0.98)
                    } else if is_previewed {
                        Color::srgb(0.46, 0.78, 1.0)
                    } else {
                        Color::srgb(0.34, 0.86, 0.94)
                    }),
                ));
                slot_root.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(4.0),
                        top: Val::Px(4.0),
                        width: Val::Percent(91.0),
                        height: Val::Percent(80.0),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.03, 0.08, 0.1, 0.55)),
                    BorderColor::all(accent.with_alpha(0.35)),
                ));
                slot_root.spawn((
                    ShipbuildingSlotScanline {
                        period_scale: 0.08 + hash_phase(&slot.slot_id) * 0.06,
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(8.0),
                        top: Val::Percent(10.0),
                        width: Val::Percent(84.0),
                        height: Val::Px(3.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.72, 0.96, 1.0, 0.08)),
                ));
            } else {
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
                    BackgroundColor(Color::srgba(0.02, 0.04, 0.065, 0.9)),
                    BorderColor::all(Color::srgba(0.16, 0.34, 0.4, 0.35)),
                ));
                spawn_dashed_outline(slot_root, accent);
            }

            spawn_chamfer_notches(slot_root);
            spawn_slot_greebles(slot_root);

            if !filled {
                slot_root.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        right: Val::Px(2.0),
                        top: Val::Px(8.0),
                        ..default()
                    },
                    Text::new(vertical_watermark(slot.category)),
                    TextFont {
                        font_size: 8.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.5, 0.85, 0.95, 0.12)),
                ));
            }

            slot_root
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(11.0),
                        top: Val::Px(5.0),
                        width: Val::Px((width - 24.0).max(88.0)),
                        height: Val::Px((height - 10.0).max(46.0)),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(1.0),
                        ..default()
                    },
                ))
                .with_children(|content| {
                    content
                        .spawn((
                            Node {
                                width: Val::Percent(100.0),
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::SpaceBetween,
                                ..default()
                            },
                        ))
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
                                Text::new(format!("{} [{}]", ascii_category_tag(slot.category), size_badge(&slot.size))),
                                TextFont {
                                    font_size: 8.5,
                                    ..default()
                                },
                                TextColor(accent),
                            ));
                        });

                    content.spawn((
                        Text::new(if filled {
                            module_name
                        } else {
                            format!("{} socket", category_text.to_ascii_uppercase())
                        }),
                        TextFont {
                            font_size: if filled { 8.0 } else { 7.5 },
                            ..default()
                        },
                        TextColor(if filled {
                            Color::srgb(0.78, 0.86, 0.9)
                        } else {
                            Color::srgba(0.7, 0.85, 0.92, 0.32)
                        }),
                    ));

                    content
                        .spawn((
                            Node {
                                width: Val::Percent(100.0),
                                margin: UiRect::top(Val::Px(2.0)),
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::End,
                                ..default()
                            },
                        ))
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

            slot_root.spawn((
                ShipbuildingSlotOrbitRunner {
                    slot_id: slot.slot_id.clone(),
                    width,
                    height,
                    phase_offset: hash_phase(&slot.slot_id),
                },
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Px(12.0),
                    height: Val::Px(2.0),
                    display: if is_selected { Display::Flex } else { Display::None },
                    ..default()
                },
                BackgroundColor(Color::srgba(0.9, 1.0, 1.0, 0.95)),
            ));
        });
}

fn spawn_dashed_outline(parent: &mut ChildSpawnerCommands, border: Color) {
    for (left, top, width, height) in [
        (6.0, 0.0, 18.0, 2.0),
        (32.0, 0.0, 18.0, 2.0),
        (58.0, 0.0, 18.0, 2.0),
        (84.0, 0.0, 12.0, 2.0),
        (0.0, 8.0, 2.0, 14.0),
        (0.0, 30.0, 2.0, 14.0),
        (98.0, 8.0, 2.0, 14.0),
        (98.0, 30.0, 2.0, 14.0),
        (6.0, 98.0, 18.0, 2.0),
        (32.0, 98.0, 18.0, 2.0),
        (58.0, 98.0, 18.0, 2.0),
        (84.0, 98.0, 12.0, 2.0),
    ] {
        parent.spawn((
            ShipbuildingSlotDash,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(left),
                top: Val::Percent(top),
                width: Val::Percent(width),
                height: Val::Px(height),
                ..default()
            },
            BackgroundColor(Color::srgba(border.to_srgba().red, border.to_srgba().green, border.to_srgba().blue, 0.72)),
        ));
    }
}

fn spawn_chamfer_notches(parent: &mut ChildSpawnerCommands) {
    for (left, top) in [(0.0, 0.0), (94.0, 90.0)] {
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(left),
                top: Val::Percent(top),
                width: Val::Px(8.0),
                height: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.02, 0.035, 0.96)),
        ));
    }
}

fn spawn_slot_greebles(parent: &mut ChildSpawnerCommands) {
    for top in [22.0, 48.0, 74.0] {
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(10.0),
                top: Val::Percent(top),
                width: Val::Percent(72.0),
                height: Val::Px(1.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.44, 0.72, 0.84, 0.05)),
        ));
    }

    for left in [22.0, 48.0, 74.0] {
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(left),
                top: Val::Percent(12.0),
                width: Val::Px(1.0),
                height: Val::Percent(70.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.44, 0.72, 0.84, 0.04)),
        ));
    }
}

fn slot_accent_color(category: ShipModuleCategory) -> Color {
    match category {
        ShipModuleCategory::FlightSystems => Color::srgb(1.0, 0.62, 0.28),
        ShipModuleCategory::PowerThermal
        | ShipModuleCategory::Sensors
        | ShipModuleCategory::UtilitySupport
        | ShipModuleCategory::SpecialScience => Color::srgb(0.26, 0.86, 1.0),
        ShipModuleCategory::Weapons
        | ShipModuleCategory::FireControl
        | ShipModuleCategory::ArmorDefense
        | ShipModuleCategory::ElectronicWarfare => Color::srgb(1.0, 0.34, 0.34),
        ShipModuleCategory::FuelStorage => Color::srgb(0.56, 0.92, 0.66),
        ShipModuleCategory::CrewSystems => Color::srgb(0.9, 0.84, 0.58),
        ShipModuleCategory::ConstructionISRU => Color::srgb(0.96, 0.72, 0.38),
    }
}

fn ascii_category_tag(category: ShipModuleCategory) -> &'static str {
    match category {
        ShipModuleCategory::FlightSystems => "DRV",
        ShipModuleCategory::PowerThermal => "PWR",
        ShipModuleCategory::FuelStorage => "FUEL",
        ShipModuleCategory::Weapons => "WPN",
        ShipModuleCategory::FireControl => "FCS",
        ShipModuleCategory::Sensors => "SNS",
        ShipModuleCategory::ArmorDefense => "DEF",
        ShipModuleCategory::CrewSystems => "CMD",
        ShipModuleCategory::UtilitySupport => "AUX",
        ShipModuleCategory::ConstructionISRU => "ISRU",
        ShipModuleCategory::ElectronicWarfare => "EW",
        ShipModuleCategory::SpecialScience => "SCI",
    }
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
        return format!("{} {}", if slot.required { "REQ" } else { "AUX" }, pip_bar(0, 5));
    };

    let power_metric = module.power_draw_mw.abs().max(module.power_generation_mw.abs() * 0.5);
    let power_pips = pip_count(power_metric, [0.5, 2.0, 6.0, 15.0]);
    let heat_alert = if module.category == ShipModuleCategory::PowerThermal && module.power_generation_mw > 0.0 {
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

fn vertical_watermark(category: ShipModuleCategory) -> String {
    let label = match category {
        ShipModuleCategory::FlightSystems => "THRUST",
        ShipModuleCategory::PowerThermal => "POWER",
        ShipModuleCategory::FuelStorage => "FUEL",
        ShipModuleCategory::Weapons => "WEAPON",
        ShipModuleCategory::FireControl => "TARGET",
        ShipModuleCategory::Sensors => "SENSOR",
        ShipModuleCategory::ArmorDefense => "DEFENSE",
        ShipModuleCategory::CrewSystems => "CREW",
        ShipModuleCategory::UtilitySupport => "UTILITY",
        ShipModuleCategory::ConstructionISRU => "ISRU",
        ShipModuleCategory::ElectronicWarfare => "EWAR",
        ShipModuleCategory::SpecialScience => "SCIENCE",
    };
    label.chars().map(|character| character.to_string()).collect::<Vec<_>>().join("\n")
}

fn hash_phase(value: &str) -> f32 {
    let hash = value.bytes().fold(0_u32, |acc, byte| acc.wrapping_mul(31).wrapping_add(byte as u32));
    (hash % 100) as f32 / 100.0
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
        .spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                ..default()
            },
        ))
        .with_children(|column| {
            column
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(6.0),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                ))
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
        .spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(4.0),
                ..default()
            },
        ))
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
                    chip.spawn(text_block(value.clone(), 10.5, Color::srgb(0.84, 0.9, 0.94)));
                    if !delta.is_empty() {
                        chip.spawn(text_block(delta.clone(), 10.5, Color::srgb(0.68, 0.9, 0.76)));
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

fn slot_indicator_legend(slot: &HullSlotDefinition) -> String {
    format!(
        "{}={} | [{}]={} slot | REQ/AUX=required vs optional | PWR bar=power intensity | THM=thermal state | MASS/NET=module mass and net power",
        ascii_category_tag(slot.category),
        slot.category.display_name(),
        size_badge(&slot.size),
        slot.size,
    )
}

fn module_indicator_legend(module: &crate::shipbuilding::ShipModuleDefinition) -> String {
    format!(
        "{}={} | [{}]={} slot | PWR ####.=power load band | THM OK/HOT=thermal state | MASS=tons | NET=generated minus drawn power",
        ascii_category_tag(module.category),
        module.category.display_name(),
        size_badge(&module.size),
        module.size,
    )
}

fn slot_orbit_perimeter(width: f32, height: f32) -> f32 {
    ((width - 8.0) * 2.0 + (height - 8.0) * 2.0).max(1.0)
}

fn orbit_runner_rect(distance: f32, width: f32, height: f32) -> (f32, f32, f32, f32) {
    let left_edge = 3.0;
    let top_edge = 3.0;
    let right_edge = (width - 25.0).max(left_edge);
    let bottom_edge = (height - 5.0).max(top_edge);
    let top_length = (right_edge - left_edge).max(1.0);
    let side_length = (bottom_edge - top_edge).max(1.0);
    let perimeter = top_length * 2.0 + side_length * 2.0;
    let d = distance % perimeter;

    if d < top_length {
        (left_edge + d, top_edge, 22.0, 2.0)
    } else if d < top_length + side_length {
        (right_edge, top_edge + (d - top_length), 2.0, 18.0)
    } else if d < top_length * 2.0 + side_length {
        (
            right_edge - (d - top_length - side_length),
            bottom_edge,
            22.0,
            2.0,
        )
    } else {
        (left_edge, bottom_edge - (d - top_length * 2.0 - side_length), 2.0, 18.0)
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

fn category_color(category: ShipModuleCategory) -> Color {
    match category {
        ShipModuleCategory::FlightSystems => Color::srgb(0.35, 0.88, 1.0),
        ShipModuleCategory::PowerThermal => Color::srgb(1.0, 0.76, 0.28),
        ShipModuleCategory::FuelStorage => Color::srgb(0.45, 0.85, 0.66),
        ShipModuleCategory::Weapons => Color::srgb(1.0, 0.46, 0.35),
        ShipModuleCategory::FireControl => Color::srgb(0.8, 0.7, 1.0),
        ShipModuleCategory::Sensors => Color::srgb(0.5, 0.92, 0.9),
        ShipModuleCategory::ArmorDefense => Color::srgb(0.86, 0.92, 1.0),
        ShipModuleCategory::CrewSystems => Color::srgb(0.85, 0.82, 0.64),
        ShipModuleCategory::UtilitySupport => Color::srgb(0.62, 0.85, 1.0),
        ShipModuleCategory::ConstructionISRU => Color::srgb(0.9, 0.72, 0.45),
        ShipModuleCategory::ElectronicWarfare => Color::srgb(1.0, 0.62, 0.78),
        ShipModuleCategory::SpecialScience => Color::srgb(0.6, 1.0, 0.8),
    }
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
                    column.spawn(hull_option_button(hull.id.clone(), hull.display_name.clone(), is_selected));
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
        .filter(|category| hull.slot_layout.iter().any(|slot| slot.category == *category))
        .collect();

    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
        ))
        .with_children(|rows| {
            for chunk in present_categories.chunks(2) {
                rows.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(4.0),
                        ..default()
                    },
                ))
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
                        row.spawn((Node { flex_grow: 1.0, ..default() },));
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
            category_color(category)
        } else {
            Color::srgb(0.22, 0.35, 0.42)
        }),
        Text::new(format!("{} {}", ascii_category_tag(category), category.display_name())),
        TextFont {
            font_size: 10.5,
            ..default()
        },
        TextColor(Color::srgb(0.88, 0.93, 0.96)),
    )
}

fn prettify_slot_name(slot_id: &str) -> String {
    slot_id
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut label = first.to_uppercase().to_string();
                    label.push_str(chars.as_str());
                    label
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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

    ui_state.selected_hull_id = Some(hull.id.clone());
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

    if let Some(slot) = hull.slot_layout.iter().find(|slot| slot.category == category) {
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

        if let Some(module) = shipbuilding_data.compatible_modules_for_slot(slot, research_state).first() {
            ui_state
                .selected_modules
                .insert(slot.slot_id.clone(), module.id.clone());
        }
    }
}

fn effective_design_name(ui_state: &ShipbuildingUiState, fallback_hull_name: &str) -> String {
    if ui_state.design_name.trim().is_empty() {
        format!("{} Prototype", fallback_hull_name)
    } else {
        ui_state.design_name.trim().to_string()
    }
}