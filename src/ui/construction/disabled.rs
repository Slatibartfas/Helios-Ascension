//! Disabled / hover / label / marquee / CTA click systems for the
//! construction UI.
//!
//! These systems share a single file because they all reason about
//! CTA disabled state, hover affordance, label colour, and the
//! subtitle marquee animation.

use bevy::prelude::*;

use crate::ui::bevy_theme::*;
use super::data::compute_colony_spare_power_mw_opt;
use super::markers::*;
use super::state::*;
use crate::ui::widgets::UiFonts;
use crate::colony::components::PendingConstructionActions;
use crate::colony::data::{parse_resource_type, BuildingsData};
use crate::economy::ResourceType;

// Hover / click effect system for the Queue CTAs.
pub fn tick_construction_cta_hover(
    mut params: ParamSet<(
        Query<
            (
                Entity,
                &Interaction,
                &mut BackgroundColor,
                &mut BorderColor,
                &mut UiTransform,
            ),
            With<ConstructionCta>,
        >,
        Query<Entity, Or<(With<ConstructionCtaDisabled>, With<ConstructionCtaBodyBlocked>)>>,
    )>,
    mut prev_state: Local<std::collections::HashMap<Entity, (Interaction, bool)>>,
) {
    let mut disabled_set: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    for entity in params.p1().iter() {
        disabled_set.insert(entity);
    }
    for (entity, interaction, mut bg, mut border, mut ui_transform) in params.p0().iter_mut() {
        let is_disabled = disabled_set.contains(&entity);
        let prev = prev_state.get(&entity).copied();
        if let Some((prev_int, prev_disabled)) = prev {
            if prev_int == *interaction && prev_disabled == is_disabled {
                continue;
            }
        }
        match interaction {
            Interaction::Pressed if !is_disabled => {
                *bg = BackgroundColor(CTA_FILL_HOVER);
                *border = BorderColor::all(CYAN);
                ui_transform.scale = Vec2::splat(0.98);
            }
            Interaction::Hovered if !is_disabled => {
                *bg = BackgroundColor(CTA_FILL_HOVER);
                *border = BorderColor::all(CYAN);
                ui_transform.scale = Vec2::splat(1.02);
            }
            Interaction::None if !is_disabled => {
                *bg = BackgroundColor(CTA_FILL);
                *border = BorderColor::all(CYAN_BORDER_STRONG);
                ui_transform.scale = Vec2::splat(1.00);
            }
            _ => {
                *bg = BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.04));
                *border = BorderColor::all(Color::srgba(
                    0.498, 0.580, 0.659, 0.40,
                ));
                ui_transform.scale = Vec2::splat(1.00);
            }
        }
        prev_state.insert(entity, (*interaction, is_disabled));
    }
}

// Animate the `UiTransform.translation` of every `SubtitleMarquee`
// track so an overflowing subtitle scrolls left/right on hover.
pub fn tick_subtitle_marquee(
    time: Res<Time>,
    text_computed: Query<&ComputedNode>,
    mut tracks: Query<(Entity, &mut SubtitleMarquee, &mut UiTransform)>,
) {
    const PIXELS_PER_SECOND: f32 = 30.0;
    let dt = time.delta_secs().min(0.1);

    for (_track_entity, mut marquee, mut ui_transform) in tracks.iter_mut() {
        let text_width = text_computed
            .get(marquee.text_node)
            .map(|c| c.content_size().x)
            .unwrap_or(0.0);
        let container_width = text_computed
            .get(marquee.clip_container)
            .map(|c| c.size().x)
            .unwrap_or(0.0);
        marquee.text_width = text_width;
        marquee.container_width = container_width;

        let overflows = text_width > container_width + 0.5;

        if overflows {
            let delta_pixels = dt * PIXELS_PER_SECOND;
            marquee.phase += delta_pixels;
            if marquee.phase >= text_width {
                marquee.phase -= text_width;
            }
        }

        let dx = -marquee.phase;
        ui_transform.translation = Val2::px(dx, 0.0);
    }
}

// Per-frame sweep that toggles the `ConstructionCtaDisabled` marker
// on every CTA based on the player's `ContextualStockpile` × multiplier.
pub fn tick_construction_cta_disabled(
    mut commands: Commands,
    buildings_data: Res<BuildingsData>,
    contextual: Res<crate::economy::ContextualStockpile>,
    ui_state: Res<ConstructionUiState>,
    ctas: Query<
        (Entity, &ConstructionCta, Has<ConstructionCtaDisabled>),
        Without<ConstructionCtaBodyBlocked>,
    >,
    local_stockpiles: Query<&crate::economy::LocalStockpile>,
) {
    let multiplier = ui_state.build_multiplier.max(1);
    let active_colony = ui_state.selected_colony;
    let local = active_colony.and_then(|e| local_stockpiles.get(e).ok());
    for (entity, cta, already_disabled) in ctas.iter() {
        let costs = buildings_data.resource_costs(&cta.building_type);
        let typed_costs: Vec<(ResourceType, f64)> = costs
            .iter()
            .filter_map(|(name, amt)| {
                parse_resource_type(name).map(|rt| (rt, amt * multiplier as f64))
            })
            .collect();
        let can_afford = typed_costs
            .iter()
            .all(|(rt, need)| contextual.get(rt) >= *need);
        let can_pay_locally = match local {
            Some(ls) => typed_costs.iter().all(|(rt, need)| ls.get(rt) >= *need),
            None => true,
        };
        let should_disable = !can_afford || !can_pay_locally;
        if should_disable && !already_disabled {
            commands.entity(entity).queue_silenced(InsertCtaDisabled);
        } else if !should_disable && already_disabled {
            commands.entity(entity).queue_silenced(RemoveCtaDisabled);
        }
    }
}

// `EntityCommand` that inserts `ConstructionCtaDisabled`.
struct InsertCtaDisabled;

impl bevy::ecs::system::EntityCommand for InsertCtaDisabled {
    fn apply(self, mut entity: bevy::ecs::world::EntityWorldMut) {
        entity.insert(ConstructionCtaDisabled);
    }
}

// `EntityCommand` that removes `ConstructionCtaDisabled`.
struct RemoveCtaDisabled;

impl bevy::ecs::system::EntityCommand for RemoveCtaDisabled {
    fn apply(self, mut entity: bevy::ecs::world::EntityWorldMut) {
        entity.remove::<ConstructionCtaDisabled>();
    }
}

// Per-frame pass that dims the Queue CTA label when the CTA is
// disabled.
pub fn tick_construction_cta_label_dim(
    cta_q: Query<
        Entity,
        Or<(With<ConstructionCtaDisabled>, With<ConstructionCtaBodyBlocked>)>,
    >,
    mut label_q: Query<
        (&ChildOf, &mut TextColor),
        With<ConstructionCtaLabelMarker>,
    >,
) {
    let mut disabled: std::collections::HashSet<Entity> =
        std::collections::HashSet::new();
    for entity in cta_q.iter() {
        disabled.insert(entity);
    }
    for (child_of, mut color) in label_q.iter_mut() {
        let target = if disabled.contains(&child_of.parent()) {
            TEXT_DIM
        } else {
            CYAN
        };
        *color = TextColor(target);
    }
}

// Refresh the card grid: despawn all existing `BuildCard` entities
// and re-spawn based on the current `ConstructionUiState`.
pub fn refresh_card_grid(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    buildings_data: Res<BuildingsData>,
    research_state: Res<crate::research::systems::ResearchState>,
    ui_state: Res<ConstructionUiState>,
    building_icons: Option<Res<BuildingIcons>>,
    resource_icons: Option<Res<crate::ui::resource_icons::ResourceIcons>>,
    card_query: Query<Entity, With<BuildCard>>,
    grid_query: Query<Entity, With<CardGrid>>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    mut tooltip_request: ResMut<crate::ui::widgets::TooltipRequest>,
) {
    for entity in card_query.iter() {
        commands.entity(entity).try_despawn();
    }
    // Clear any chip-driven tooltip: the chips we just despawned no
    // longer exist, so the Pointer<Out> observer can never fire to
    // clear the request itself. Without this guard the tooltip
    // lingers against the cursor until the next chip's Pointer<Over>
    // overwrites it.
    if tooltip_request.content.is_some() {
        tooltip_request.content = None;
        tooltip_request.hover_started_at = None;
    }
    let Ok(card_grid) = grid_query.single() else {
        return;
    };
    let body_font = fonts.body.clone();
    let body_font_medium = fonts.medium.clone();
    let mono_font = fonts.mono.clone();
    let category_idx = ui_state.selected_build_tab;
    let filter = ui_state.selected_filter;
    let multiplier = ui_state.build_multiplier;
    let spare_power_mw = compute_colony_spare_power_mw_opt(&ui_state, &colonies, Some(&buildings_data));
    for (building_type, card_data) in super::data::visible_cards(
        &buildings_data,
        &research_state,
        category_idx,
        filter,
        multiplier,
        spare_power_mw,
    ) {
        let icon_handle: Option<&Handle<Image>> = building_icons
            .as_ref()
            .and_then(|icons| icons.handles.get(&building_type));
        let empty_resource_icons = crate::ui::resource_icons::ResourceIcons::default();
        let resource_icons_ref: &crate::ui::resource_icons::ResourceIcons = resource_icons
            .as_ref()
            .map(|r: &Res<crate::ui::resource_icons::ResourceIcons>| -> &crate::ui::resource_icons::ResourceIcons { r.as_ref() })
            .unwrap_or(&empty_resource_icons);
        let card = super::cards::spawn_card(
            &mut commands,
            card_grid,
            &card_data,
            building_type,
            &body_font,
            &body_font_medium,
            &mono_font,
            icon_handle,
            resource_icons_ref,
        );
        commands.entity(card).insert(BuildCard);
    }
}

// Click handler: when the player presses the Queue button on a build
// card, push `(selected_colony, building_type)` to
// `PendingConstructionActions::start_construction`.
pub fn tick_construction_cta_click(
    interactions: Query<(Entity, &Interaction, &ConstructionCta), With<ConstructionCta>>,
    disabled: Query<
        Entity,
        Or<(With<ConstructionCtaDisabled>, With<ConstructionCtaBodyBlocked>)>,
    >,
    ui_state: Res<ConstructionUiState>,
    mut pending: ResMut<PendingConstructionActions>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut disabled_set: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    for entity in disabled.iter() {
        disabled_set.insert(entity);
    }
    crate::ui::widgets::detect_rising_edges(&mut prev, &interactions, |entity, cta| {
        if disabled_set.contains(&entity) {
            return;
        }
        let Some(colony_entity) = ui_state.selected_colony else {
            return;
        };
        let multiplier = ui_state.build_multiplier.max(1);
        for _ in 0..multiplier {
            pending
                .start_construction
                .push((colony_entity, cta.building_type));
        }
    });
}
