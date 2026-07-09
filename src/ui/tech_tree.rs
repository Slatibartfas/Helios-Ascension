use super::research_panel::{render_research_tech_tooltip_content, ActiveProjectInfo};
use super::*;

/// Direction for tech-tree keyboard navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TechNavDirection {
    Up,
    Down,
    Left,
    Right,
}

impl TechNavDirection {
    /// Build from a pressed arrow key, or `None` if the key isn't an arrow.
    pub(super) fn from_key(key: egui::Key) -> Option<Self> {
        match key {
            egui::Key::ArrowUp => Some(Self::Up),
            egui::Key::ArrowDown => Some(Self::Down),
            egui::Key::ArrowLeft => Some(Self::Left),
            egui::Key::ArrowRight => Some(Self::Right),
            _ => None,
        }
    }
}

/// Find the next tech to select given the current selection (or `None` to
/// pick a starting node) and a direction.
///
/// Pure / testable: the caller passes the full `node_positions` map (tech
/// id → node centre) and gets back the id of the best neighbour. If
/// `current_id` is `None`, the function picks the top-left-most node so
/// arrow-key navigation always starts somewhere deterministic. If the
/// `node_positions` map is empty (no techs at all), the function returns
/// `None`.
///
/// The neighbour score is `primary_dist + 0.5 * secondary_dist`: nodes
/// that are primarily in the requested direction win, but ties on the
/// primary axis are broken by the secondary axis so the player moves to
/// the closest *aligned* neighbour rather than a perpendicular one.
pub(super) fn nearest_tech_in_direction(
    node_positions: &std::collections::HashMap<String, egui::Pos2>,
    current_id: Option<&str>,
    direction: TechNavDirection,
) -> Option<String> {
    if node_positions.is_empty() {
        return None;
    }

    let current_pos = current_id.and_then(|id| node_positions.get(id));

    // No current selection → pick the top-left-most node so the player
    // always lands somewhere visible when they first press an arrow.
    let Some(current_pos) = current_pos else {
        return node_positions
            .iter()
            .min_by(|a, b| {
                a.1.y
                    .partial_cmp(&b.1.y)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(
                        a.1.x
                            .partial_cmp(&b.1.x)
                            .unwrap_or(std::cmp::Ordering::Equal),
                    )
            })
            .map(|(id, _)| id.clone());
    };

    let mut best: Option<(f32, String)> = None;
    for (id, pos) in node_positions {
        if Some(id.as_str()) == current_id {
            continue;
        }
        let dx = pos.x - current_pos.x;
        let dy = pos.y - current_pos.y;

        let (is_in_direction, primary, secondary) = match direction {
            TechNavDirection::Right => (dx > 0.0, dx, dy.abs()),
            TechNavDirection::Left => (dx < 0.0, -dx, dy.abs()),
            TechNavDirection::Down => (dy > 0.0, dy, dx.abs()),
            TechNavDirection::Up => (dy < 0.0, -dy, dx.abs()),
        };
        if !is_in_direction {
            continue;
        }
        // Equal-cost fallback: same primary distance, pick the one whose
        // y is closer to the current node's y (Left/Right) or x is
        // closer (Up/Down). Achieved by weighting secondary at 0.5 so
        // a primary-axis match wins, but a perpendicular overshoot is
        // still penalised.
        let score = primary + 0.5 * secondary;
        if best.as_ref().is_none_or(|(s, _)| score < *s) {
            best = Some((score, id.clone()));
        }
    }
    best.map(|(_, id)| id)
}

/// Render the Tech Tree tab
pub(super) fn render_tech_tree_tab(
    ui: &mut egui::Ui,
    research_state: &ResearchState,
    tech_data: &mut TechnologiesData,
    icon_textures: &HashMap<TechCategory, egui::TextureId>,
    debug_enabled: bool,
    edit_state: &mut TechTreeEditState,
    active_research: &HashMap<String, ActiveProjectInfo>,
    pending_research: &mut crate::research::PendingResearchActions,
    debug_settings: &mut crate::research::ResearchDebugSettings,
) {
    // PR-D / GRA-69 — the panel title goes through `theme::section_h1`
    // (Pattern 1 / Pattern 4 heading); the three control-hint lines
    // below are captions, not section headers, so they use the
    // `theme::caption` builder. The debug-only "right-click …" line
    // stays amber so the debug affordance still reads as such.
    theme::section_h1(ui, "Technology Tree - Graph View");
    ui.label(theme::caption(
        "Pan: Middle mouse drag | Zoom: Mouse wheel | Click: Select tech & highlight path",
    ));
    ui.label(theme::caption(
        "Arrows: Move focus | Enter: Start research on selected | Esc: Clear",
    ));
    if debug_enabled {
        ui.label(
            egui::RichText::new(
                "Right-click: Edit/delete node | Right-click empty space: Add new tech",
            )
            .small()
            .color(theme::AMBER),
        );
    }
    ui.separator();

    // Local state for pan, zoom, and selected tech (using unique ID for persistence)
    let pan_id = ui.id().with("tech_tree_pan");
    let zoom_id = ui.id().with("tech_tree_zoom");
    let sel_persist_id = ui.id().with("tech_tree_selected");

    let mut pan_offset: egui::Vec2 = ui.data_mut(|data| {
        data.get_persisted(pan_id)
            .unwrap_or(egui::Vec2::new(50.0, 50.0))
    });

    let mut zoom: f32 = ui.data_mut(|data| data.get_persisted(zoom_id).unwrap_or(1.0));

    let mut selected_tech: Option<String> = ui.data_mut(|data| data.get_persisted(sel_persist_id));

    // ---------- layout constants ----------
    let tier_spacing = 310.0 * zoom;
    let node_gap_y = 14.0 * zoom;
    let category_gap = 24.0 * zoom;
    let pane_pad = (10.0 * zoom).round();
    let pane_rounding = 6.0 * zoom;
    let label_width = (140.0 * zoom).round();

    // ---------- status line (fixed height, drawn FIRST so it reserves space at the bottom) ----------
    // We draw it at the end but must reserve its height now.
    let status_height = 26.0;

    // ---------- canvas: allocate ALL remaining space minus status ----------
    let avail = ui.available_rect_before_wrap();
    if avail.height() <= status_height + 10.0 {
        ui.label("Window too small to display tech tree");
        return;
    }
    let canvas_rect = egui::Rect::from_min_max(
        avail.min,
        egui::Pos2::new(avail.max.x, avail.max.y - status_height),
    );

    // Single response for the whole canvas – handles pan / zoom / click
    let response = ui.allocate_rect(canvas_rect, egui::Sense::click_and_drag());

    // Zoom – use pointer position directly so zooming works even when a tooltip is shown
    if ui.input(|i| {
        i.pointer
            .hover_pos()
            .is_some_and(|pos| canvas_rect.contains(pos))
    }) {
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_delta != 0.0 {
            zoom = (zoom + scroll_delta * 0.001).clamp(0.3, 3.0);
        }
    }
    // Pan (middle-click drag) – read raw pointer delta so pan works even when a tooltip is shown
    let pointer_in_canvas = ui.input(|i| {
        i.pointer
            .hover_pos()
            .is_some_and(|pos| canvas_rect.contains(pos))
    });
    if pointer_in_canvas && ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle)) {
        pan_offset += ui.input(|i| i.pointer.delta());
    }

    // Persist pan / zoom immediately
    ui.data_mut(|data| {
        data.insert_persisted(pan_id, pan_offset);
        data.insert_persisted(zoom_id, zoom);
    });

    // Clipped painter so nothing bleeds outside the canvas
    let clip = ui.clip_rect().intersect(canvas_rect);
    let painter = ui.painter().with_clip_rect(clip);

    // ---------- compute uniform node size ----------
    // Use a fixed node size based on zoom so all boxes are identical.
    // Two rows: row 1 = icon + name, row 2 = research cost
    let font_name = egui::FontId::proportional((12.0 * zoom).round());
    let font_cost = egui::FontId::proportional((10.0 * zoom).round());
    let icon_sz = (16.0 * zoom).round();
    let icon_pad = (4.0 * zoom).round();
    let h_pad = (8.0 * zoom).round();
    let v_pad = (6.0 * zoom).round();
    let row_gap = (3.0 * zoom).round();

    // Measure the widest tech name to determine uniform width
    let mut max_name_w: f32 = 0.0;
    let mut max_cost_w: f32 = 0.0;
    for tech in tech_data.technologies.values() {
        let g = painter.layout_no_wrap(tech.name.clone(), font_name.clone(), egui::Color32::WHITE);
        max_name_w = max_name_w.max(g.size().x);
        let cost_text = format!("{:.0} RP", tech.research_cost);
        let g2 = painter.layout_no_wrap(cost_text, font_cost.clone(), egui::Color32::WHITE);
        max_cost_w = max_cost_w.max(g2.size().x);
    }
    // Row heights (approximate from font size)
    let name_row_h = font_name.size * 1.3;
    let cost_row_h = font_cost.size * 1.3;

    let node_w = (icon_sz + icon_pad + max_name_w.max(max_cost_w) + h_pad * 2.0).round();
    let node_h = (v_pad + name_row_h + row_gap + cost_row_h + v_pad).round();

    // ---------- compute node positions: horizontal category bands ----------
    // Layout: each category is a horizontal band (row).  Within each band,
    // tiers run left-to-right as columns.  Multiple techs in the same
    // (category, tier) cell are stacked vertically within that band.
    let mut node_positions: HashMap<String, egui::Pos2> = HashMap::new();

    // Collect unique tiers (sorted)
    let mut tier_set: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for tech in tech_data.technologies.values() {
        tier_set.insert(tech.tier);
    }
    let tiers: Vec<u32> = tier_set.into_iter().collect();
    let tier_index_map: HashMap<u32, usize> =
        tiers.iter().enumerate().map(|(i, &t)| (t, i)).collect();

    // Group techs: category -> tier -> Vec<tech>
    let mut techs_by_cat_tier: std::collections::BTreeMap<
        u8,
        std::collections::BTreeMap<u32, Vec<&crate::research::types::Technology>>,
    > = std::collections::BTreeMap::new();
    for tech in tech_data.technologies.values() {
        techs_by_cat_tier
            .entry(tech.category as u8)
            .or_default()
            .entry(tech.tier)
            .or_default()
            .push(tech);
    }
    // Sort techs within each cell alphabetically for deterministic layout
    for cat_tiers in techs_by_cat_tier.values_mut() {
        for cell_techs in cat_tiers.values_mut() {
            cell_techs.sort_by_key(|t| &t.name);
        }
    }

    // Compute height of each category band (max stacked techs across all tiers)
    // and record category row Y start positions
    struct CategoryBand {
        category: TechCategory,
        y_start: f32,
        height: f32,
    }
    let mut category_bands: Vec<CategoryBand> = Vec::new();
    let origin_x = canvas_rect.left() + pan_offset.x + label_width;
    let mut current_y = canvas_rect.top() + pan_offset.y;

    let categories = TechCategory::all();
    for &cat in categories {
        let cat_key = cat as u8;
        let max_stack = if let Some(cat_tiers) = techs_by_cat_tier.get(&cat_key) {
            cat_tiers.values().map(|v| v.len()).max().unwrap_or(0)
        } else {
            0
        };
        if max_stack == 0 {
            continue; // skip empty categories
        }
        let band_content_h =
            max_stack as f32 * node_h + (max_stack as f32 - 1.0).max(0.0) * node_gap_y;
        let band_h = band_content_h + pane_pad * 2.0;

        category_bands.push(CategoryBand {
            category: cat,
            y_start: current_y,
            height: band_h,
        });
        current_y += band_h + category_gap;
    }

    // Place nodes within each category band
    for band in &category_bands {
        let cat_key = band.category as u8;
        if let Some(cat_tiers) = techs_by_cat_tier.get(&cat_key) {
            for (&tier, cell_techs) in cat_tiers {
                let tier_idx = tier_index_map.get(&tier).copied().unwrap_or(0);
                let col_x = origin_x + (tier_idx as f32) * tier_spacing;
                // Center the stack vertically within the band
                let stack_h = cell_techs.len() as f32 * node_h
                    + (cell_techs.len() as f32 - 1.0).max(0.0) * node_gap_y;
                let stack_y_start =
                    band.y_start + pane_pad + (band.height - pane_pad * 2.0 - stack_h) / 2.0;

                for (i, tech) in cell_techs.iter().enumerate() {
                    let node_top = stack_y_start + i as f32 * (node_h + node_gap_y);
                    let center_x = col_x + node_w / 2.0;
                    let center_y = node_top + node_h / 2.0;
                    node_positions.insert(tech.id.clone(), egui::Pos2::new(center_x, center_y));
                }
            }
        }
    }

    // Compute total width spanned by tier columns for pane drawing
    let total_tier_width = if tiers.is_empty() {
        node_w
    } else {
        (tiers.len() as f32 - 1.0) * tier_spacing + node_w
    };

    // ---------- draw category background panes (horizontal bands) ----------
    for band in &category_bands {
        let cat_color = tech_category_color(band.category);
        let bg_color = theme::with_alpha(cat_color, 18);
        let border_color = theme::with_alpha(cat_color, 40);
        let pane_rect = egui::Rect::from_min_size(
            egui::Pos2::new(origin_x - pane_pad, band.y_start),
            egui::Vec2::new(total_tier_width + pane_pad * 2.0, band.height),
        );
        painter.rect_filled(pane_rect, pane_rounding, bg_color);
        painter.rect_stroke(
            pane_rect,
            pane_rounding,
            egui::Stroke::new(1.0 * zoom, border_color),
            egui::StrokeKind::Outside,
        );

        // Category label on the left: icon + stacked word lines
        let cat_icon = band.category.icon();
        let cat_name = band.category.display_name().to_uppercase();

        // Fixed icon size for consistency across variable-height category panes
        let icon_font_size = (22.0 * zoom).round();
        let font_icon_large = egui::FontId::proportional(icon_font_size);
        let font_cat_word = egui::FontId::proportional((11.0 * zoom).round());

        // Split name into words, one per line
        let words: Vec<&str> = cat_name.split_whitespace().collect();
        let line_spacing = font_cat_word.size * 1.25;
        let text_block_h = words.len() as f32 * line_spacing;
        let gap_between = (4.0 * zoom).round();

        // Total height of the content block
        let total_h = icon_font_size + gap_between + text_block_h;

        // Center within the band
        let band_center_y = band.y_start + band.height / 2.0;
        let block_top = band_center_y - total_h / 2.0;
        let label_center_x = origin_x - pane_pad - label_width / 2.0;

        // Icon
        painter.text(
            egui::Pos2::new(label_center_x, block_top + icon_font_size / 2.0),
            egui::Align2::CENTER_CENTER,
            cat_icon,
            font_icon_large,
            cat_color,
        );

        // Word-per-line text
        let text_top = block_top + icon_font_size + gap_between;
        for (i, word) in words.iter().enumerate() {
            painter.text(
                egui::Pos2::new(
                    label_center_x,
                    text_top + i as f32 * line_spacing + line_spacing / 2.0,
                ),
                egui::Align2::CENTER_CENTER,
                *word,
                font_cat_word.clone(),
                theme::with_alpha(cat_color, 200),
            );
        }
    }

    // ---------- draw tier column headers ----------
    let header_y = canvas_rect.top() + pan_offset.y - (22.0 * zoom);
    let font_header = egui::FontId::proportional((15.0 * zoom).round());
    for (i, tier) in tiers.iter().enumerate() {
        let col_x = origin_x + (i as f32) * tier_spacing + node_w / 2.0;
        painter.text(
            egui::Pos2::new(col_x, header_y),
            egui::Align2::CENTER_BOTTOM,
            format!("Tier {}", tier),
            font_header.clone(),
            theme::TEXT_DIM,
        );
    }

    // ---------- prerequisite highlight path ----------
    let mut path_techs = std::collections::HashSet::new();
    if let Some(ref sel_id) = selected_tech {
        let mut to_process = vec![sel_id.clone()];
        path_techs.insert(sel_id.clone());
        while let Some(cur) = to_process.pop() {
            if let Some(tech) = tech_data.technologies.get(&cur) {
                for prereq_id in &tech.prerequisites {
                    if path_techs.insert(prereq_id.clone()) {
                        to_process.push(prereq_id.clone());
                    }
                }
            }
        }
    }

    // ---------- draw connection lines (cubic bezier) ----------
    // Connect right edge of prerequisite to left edge of dependent
    for tech in tech_data.technologies.values() {
        if let Some(tech_center) = node_positions.get(&tech.id) {
            for prereq_id in &tech.prerequisites {
                if let Some(prereq_center) = node_positions.get(prereq_id) {
                    let is_in_path =
                        path_techs.contains(&tech.id) && path_techs.contains(prereq_id);
                    let is_prereq_unlocked = research_state.is_unlocked(prereq_id);
                    let line_color = if is_in_path {
                        theme::PREREQ_IN_PATH
                    } else if is_prereq_unlocked {
                        theme::PREREQ_UNLOCKED
                    } else {
                        theme::PREREQ_DEFAULT
                    };
                    let w = if is_in_path { 2.5 * zoom } else { 1.0 * zoom };
                    // From right edge of prereq to left edge of tech
                    let from = egui::Pos2::new(prereq_center.x + node_w / 2.0, prereq_center.y);
                    let to = egui::Pos2::new(tech_center.x - node_w / 2.0, tech_center.y);
                    // Cubic bezier with horizontal tangents for a smooth S-curve
                    let mid_x = (from.x + to.x) * 0.5;
                    let bezier = egui::epaint::CubicBezierShape::from_points_stroke(
                        [
                            from,
                            egui::Pos2::new(mid_x, from.y),
                            egui::Pos2::new(mid_x, to.y),
                            to,
                        ],
                        false,
                        egui::Color32::TRANSPARENT,
                        egui::Stroke::new(w, line_color),
                    );
                    painter.add(bezier);
                }
            }
        }
    }

    // ---------- draw nodes & collect hit-test rects ----------
    // We do NOT call ui.allocate_rect for each node (that was the bug).
    // Instead we paint directly and do manual hit-testing against the pointer.
    let pointer_pos = ui.input(|i| i.pointer.interact_pos());
    let pointer_clicked = response.clicked();
    let pointer_right_clicked = response.clicked_by(egui::PointerButton::Secondary);
    let mut hovered_tech_id: Option<String> = None;
    let mut clicked_tech_id: Option<String> = None;
    let mut right_clicked_tech_id: Option<String> = None;
    // We need to collect hovered rect for tooltip
    let mut hovered_rect: Option<egui::Rect> = None;

    let unlocked_ids: Vec<_> = research_state
        .unlocked_technologies
        .iter()
        .cloned()
        .collect();

    // ---------- keyboard navigation ----------
    // Arrow keys move the selection spatially between tech nodes. Enter
    // starts research on the selected tech (mirrors the tooltip button).
    // Skipped when a text field has focus (edit dialog, prereq filter,
    // etc.) or when no techs are present.
    if !node_positions.is_empty()
        && !ui.ctx().wants_keyboard_input()
        && edit_state.context_menu.is_none()
    {
        let nav_dir: Option<TechNavDirection> = ui.input_mut(|i| {
            [
                egui::Key::ArrowUp,
                egui::Key::ArrowDown,
                egui::Key::ArrowLeft,
                egui::Key::ArrowRight,
            ]
            .iter()
            .find_map(|k| {
                if i.consume_key(egui::Modifiers::NONE, *k) {
                    TechNavDirection::from_key(*k)
                } else {
                    None
                }
            })
        });
        if let Some(dir) = nav_dir {
            if let Some(next_id) =
                nearest_tech_in_direction(&node_positions, selected_tech.as_deref(), dir)
            {
                selected_tech = Some(next_id);
            }
        }

        // Enter starts research on the currently selected tech, mirroring
        // the "🔬 Start Research" button in the node tooltip. Same gate as
        // the arrow keys so a text-editor focus swallows the press first.
        let enter_pressed =
            ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        if enter_pressed {
            if let Some(ref sel_id) = selected_tech {
                if let Some(sel_tech) = tech_data.technologies.get(sel_id) {
                    let already_unlocked = research_state.is_unlocked(&sel_tech.id);
                    let already_researching = active_research.contains_key(&sel_tech.id);
                    if !already_unlocked
                        && !already_researching
                        && tech_data.check_prerequisites(&sel_tech.id, &unlocked_ids)
                    {
                        pending_research.start_research.push(sel_tech.id.clone());
                        pending_research.navigate_to_available_tab = true;
                    }
                }
            }
        }
    }

    for (tech_id, center) in &node_positions {
        if let Some(tech) = tech_data.technologies.get(tech_id) {
            let is_unlocked = research_state.is_unlocked(&tech.id);
            let is_researching = active_research.contains_key(&tech.id);
            let research_progress = active_research
                .get(&tech.id)
                .map(|info| info.progress_percent);
            let can_research = !is_unlocked
                && !is_researching
                && tech_data.check_prerequisites(&tech.id, &unlocked_ids);
            let is_in_path = path_techs.contains(&tech.id);
            let is_selected = selected_tech.as_ref() == Some(&tech.id);

            // Node fill color — use darker/muted tones so white text is always readable
            let node_color =
                theme::tech_node_color(is_in_path, is_unlocked, is_researching, can_research);

            let category_color = tech_category_color(tech.category);

            // Build node rect from center
            let node_rect = egui::Rect::from_center_size(
                egui::Pos2::new(center.x.round(), center.y.round()),
                egui::Vec2::new(node_w, node_h),
            );

            // --- paint background ---
            let rounding = 4.0 * zoom;
            painter.rect_filled(node_rect, rounding, node_color);

            // Border — thicker if selected or in path
            let border_w = if is_selected {
                3.5 * zoom
            } else if is_in_path {
                2.5 * zoom
            } else {
                1.5 * zoom
            };
            painter.rect_stroke(
                node_rect,
                rounding,
                egui::Stroke::new(border_w, category_color),
                egui::StrokeKind::Outside,
            );

            // --- row 1: icon + name (left-aligned) ---
            let text_color = if is_in_path {
                egui::Color32::WHITE
            } else if is_unlocked {
                theme::TECH_TEXT_UNLOCKED
            } else if can_research {
                theme::TECH_TEXT_AVAILABLE
            } else {
                theme::TEXT_DIM
            };

            let row1_y = (node_rect.top() + v_pad + name_row_h / 2.0).round();
            let content_x = (node_rect.left() + h_pad).round();

            // Icon
            if let Some(tex) = icon_textures.get(&tech.category) {
                let ir = egui::Rect::from_min_size(
                    egui::Pos2::new(content_x, (row1_y - icon_sz / 2.0).round()),
                    egui::Vec2::splat(icon_sz),
                );
                painter.image(
                    *tex,
                    ir,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                    category_color,
                );
            }

            // Name text
            let name_x = (content_x + icon_sz + icon_pad).round();
            painter.text(
                egui::Pos2::new(name_x, row1_y),
                egui::Align2::LEFT_CENTER,
                &tech.name,
                font_name.clone(),
                text_color,
            );

            // --- row 2: research cost / progress (left-aligned, dimmer) ---
            let row2_y =
                (node_rect.top() + v_pad + name_row_h + row_gap + cost_row_h / 2.0).round();
            let (cost_text, cost_color) = if is_unlocked {
                ("✔ Researched".to_string(), theme::GREEN)
            } else if let Some(pct) = research_progress {
                (
                    format!("⏳ {:.0}%  ({:.0} RP)", pct * 100.0, tech.research_cost),
                    theme::RP_BLUE,
                )
            } else {
                (format!("{:.0} RP", tech.research_cost), theme::TEXT_VALUE)
            };
            painter.text(
                egui::Pos2::new(name_x, row2_y),
                egui::Align2::LEFT_CENTER,
                &cost_text,
                font_cost.clone(),
                cost_color,
            );

            // --- progress bar for actively researching techs ---
            if let Some(pct) = research_progress {
                let bar_h = (3.0 * zoom).max(1.0);
                let bar_rect = egui::Rect::from_min_size(
                    egui::Pos2::new(node_rect.left() + 2.0, node_rect.bottom() - bar_h - 1.0),
                    egui::Vec2::new((node_rect.width() - 4.0) * pct, bar_h),
                );
                painter.rect_filled(bar_rect, 0.0, theme::RP_BLUE);
                // bg track
                let track_rect = egui::Rect::from_min_size(
                    egui::Pos2::new(
                        node_rect.left() + 2.0 + (node_rect.width() - 4.0) * pct,
                        node_rect.bottom() - bar_h - 1.0,
                    ),
                    egui::Vec2::new((node_rect.width() - 4.0) * (1.0 - pct), bar_h),
                );
                painter.rect_filled(track_rect, 0.0, theme::SURFACE);
            }

            // --- hit-test ---
            if let Some(pp) = pointer_pos {
                if node_rect.contains(pp) && canvas_rect.contains(pp) {
                    hovered_tech_id = Some(tech.id.clone());
                    hovered_rect = Some(node_rect);
                    if pointer_clicked {
                        clicked_tech_id = Some(tech.id.clone());
                    }
                    if pointer_right_clicked {
                        right_clicked_tech_id = Some(tech.id.clone());
                    }
                }
            }
        }
    }

    // Handle click – toggle selection
    if let Some(cid) = clicked_tech_id {
        if selected_tech.as_ref() == Some(&cid) {
            selected_tech = None;
        } else {
            selected_tech = Some(cid);
        }
    } else if pointer_clicked {
        // Clicked on empty space (not on any node) – clear selection
        selected_tech = None;
    }

    // Handle right-click – open context menu (debug mode only)
    if debug_enabled && pointer_right_clicked {
        if let Some(pp) = pointer_pos {
            if canvas_rect.contains(pp) {
                edit_state.context_menu = Some(ContextMenuState {
                    pos: (pp.x, pp.y),
                    tech_id: right_clicked_tech_id.clone(),
                });
            }
        }
    }

    // ---------- Debug context menu ----------
    if debug_enabled {
        let mut close_menu = false;
        if let Some(ref ctx_menu) = edit_state.context_menu.clone() {
            let menu_pos = egui::Pos2::new(ctx_menu.pos.0, ctx_menu.pos.1);
            egui::Area::new(ui.id().with("tech_ctx_menu"))
                .fixed_pos(menu_pos)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::menu(ui.style())
                        .inner_margin(4.0)
                        .show(ui, |ui| {
                            ui.set_min_width(160.0);
                            if let Some(ref tid) = ctx_menu.tech_id {
                                // Right-clicked on a node
                                ui.label(
                                    egui::RichText::new(format!("Tech: {}", tid))
                                        .strong()
                                        .small(),
                                );
                                ui.separator();
                                if ui.button("✏ Edit Technology").clicked() {
                                    if let Some(tech) = tech_data.technologies.get(tid) {
                                        edit_state.editing = Some(TechEditData::from_tech(tech));
                                    }
                                    close_menu = true;
                                }
                                if ui.button("🗑 Delete Technology").clicked() {
                                    edit_state.delete_confirm = Some(tid.clone());
                                    close_menu = true;
                                }
                            } else {
                                // Right-clicked on empty space
                                ui.label(egui::RichText::new("Tech Tree").strong().small());
                                ui.separator();
                                if ui.button("➕ Add New Technology").clicked() {
                                    edit_state.adding = Some(TechEditData::new_blank());
                                    close_menu = true;
                                }
                            }
                            if ui.button("✖ Close").clicked() {
                                close_menu = true;
                            }
                        });
                });

            // Close menu if clicked elsewhere
            let any_click = ui.input(|i| i.pointer.any_pressed());
            if any_click && !close_menu {
                // Check if the click was outside the menu area (approximate)
                if let Some(pp) = pointer_pos {
                    let menu_rect =
                        egui::Rect::from_min_size(menu_pos, egui::Vec2::new(170.0, 100.0));
                    if !menu_rect.contains(pp) {
                        close_menu = true;
                    }
                }
            }
        }
        if close_menu {
            edit_state.context_menu = None;
        }

        // ---------- Delete confirmation dialog ----------
        let mut do_delete: Option<String> = None;
        let mut cancel_delete = false;
        if let Some(ref del_id) = edit_state.delete_confirm.clone() {
            let tech_name = tech_data
                .technologies
                .get(del_id)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| del_id.clone());
            egui::Window::new("Confirm Delete")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label(format!("Delete technology \"{}\" ({})?", tech_name, del_id));
                    ui.label(
                        egui::RichText::new(
                            "This will also remove it from all prerequisite lists.",
                        )
                        .small()
                        .color(theme::AMBER),
                    );
                    ui.add_space(theme::Spacing::sm);
                    ui.horizontal(|ui| {
                        if ui.button("🗑 Delete").clicked() {
                            do_delete = Some(del_id.clone());
                        }
                        if ui.button("Cancel").clicked() {
                            cancel_delete = true;
                        }
                    });
                });
        }
        if cancel_delete {
            edit_state.delete_confirm = None;
        }
        if let Some(del_id) = do_delete {
            // Remove the technology
            tech_data.technologies.remove(&del_id);
            // Remove from all prerequisite lists
            for tech in tech_data.technologies.values_mut() {
                tech.prerequisites.retain(|p| p != &del_id);
            }
            // Clear selection if it was the deleted tech
            if selected_tech.as_ref() == Some(&del_id) {
                selected_tech = None;
            }
            edit_state.delete_confirm = None;
            save_technologies_to_file(tech_data);
        }

        // ---------- Edit Technology dialog ----------
        render_tech_edit_dialog(ui, tech_data, edit_state, false);

        // ---------- Add Technology dialog ----------
        render_tech_edit_dialog(ui, tech_data, edit_state, true);
    }

    // Show tooltip for hovered or selected node
    // Use a tooltip Window instead of show_tooltip_at so the user can interact with it
    let tooltip_hold_id = ui.id().with("tech_tooltip_hold");
    let now = ui.input(|i| i.time);
    let pointer_hover_pos = ui.input(|i| i.pointer.hover_pos());

    if let Some((held_id, _hold_until, held_rect)) =
        ui.data_mut(|data| data.get_temp::<(String, f64, egui::Rect)>(tooltip_hold_id))
    {
        let held_tooltip_pos = egui::pos2(held_rect.right() + 4.0, held_rect.top());
        let held_tooltip_rect = egui::Rect::from_min_max(
            egui::pos2(held_tooltip_pos.x - 2.0, held_tooltip_pos.y - 2.0),
            egui::pos2(held_tooltip_pos.x + 390.0, held_tooltip_pos.y + 430.0),
        );
        let pointer_inside_held_tooltip =
            pointer_hover_pos.is_some_and(|pos| held_tooltip_rect.contains(pos));

        if pointer_inside_held_tooltip {
            hovered_tech_id = None;
            hovered_rect = None;
            let hold_until = now + 0.9;
            ui.data_mut(|data| {
                data.insert_temp(tooltip_hold_id, (held_id, hold_until, held_rect));
            });
        }
    }

    if let (Some(id), Some(rect)) = (&hovered_tech_id, hovered_rect) {
        ui.data_mut(|data| {
            data.insert_temp(tooltip_hold_id, (id.clone(), now + 0.9, rect));
        });
    }

    let mut tooltip_tech_id = hovered_tech_id.clone().or_else(|| selected_tech.clone());
    let mut tooltip_rect = if hovered_tech_id.is_some() {
        hovered_rect
    } else {
        // Use the selected node's rect if we have it
        selected_tech.as_ref().and_then(|sel_id| {
            node_positions.get(sel_id).map(|center| {
                egui::Rect::from_center_size(
                    egui::Pos2::new(center.x, center.y),
                    egui::Vec2::new(node_w, node_h),
                )
            })
        })
    };

    if tooltip_tech_id.is_none() {
        if let Some((held_id, mut hold_until, held_rect)) =
            ui.data_mut(|data| data.get_temp::<(String, f64, egui::Rect)>(tooltip_hold_id))
        {
            let tooltip_pos = egui::pos2(held_rect.right() + 4.0, held_rect.top());
            let hover_bridge = egui::Rect::from_min_max(
                egui::pos2(held_rect.right() - 8.0, held_rect.top() - 20.0),
                egui::pos2(tooltip_pos.x + 390.0, tooltip_pos.y + 430.0),
            );
            let pointer_in_bridge = pointer_hover_pos.is_some_and(|pos| hover_bridge.contains(pos));

            if now <= hold_until || pointer_in_bridge {
                if pointer_in_bridge {
                    hold_until = now + 0.9;
                }
                ui.data_mut(|data| {
                    data.insert_temp(tooltip_hold_id, (held_id.clone(), hold_until, held_rect));
                });
                tooltip_tech_id = Some(held_id);
                tooltip_rect = Some(held_rect);
            } else {
                ui.data_mut(|data| {
                    data.remove::<(String, f64, egui::Rect)>(tooltip_hold_id);
                });
            }
        }
    }

    if let (Some(ref tid), Some(tr)) = (&tooltip_tech_id, tooltip_rect) {
        if let Some(tech) = tech_data.technologies.get(tid) {
            let is_researching = active_research.contains_key(&tech.id);
            let can_research = !research_state.is_unlocked(&tech.id)
                && !is_researching
                && tech_data.check_prerequisites(&tech.id, &unlocked_ids);

            let tooltip_pos = egui::pos2(tr.right() + 4.0, tr.top());

            egui::Window::new("tech_node_tooltip")
                .id(ui.id().with("tech_tooltip_win"))
                .fixed_pos(tooltip_pos)
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .frame(egui::Frame::popup(ui.ctx().style().as_ref())
                    .fill(theme::TOOLTIP_BG_ALT)
                    .stroke(egui::Stroke::new(2.0_f32, tech_category_color(tech.category))))
                .show(ui.ctx(), |ui| {
                    render_research_tech_tooltip_content(
                        ui,
                        tech,
                        tech_data,
                        research_state,
                        Some(icon_textures),
                        active_research.get(&tech.id),
                    );
                    if !is_researching && can_research {
                        ui.add_space(5.0);
                        ui.separator();
                        if ui.button("🔬 Start Research").clicked() {
                            pending_research.start_research.push(tech.id.clone());
                            pending_research.navigate_to_available_tab = true;
                        }
                    }
                    if debug_enabled {
                        ui.add_space(5.0);
                        ui.separator();
                        ui.label(egui::RichText::new("🐛 Debug").small().color(theme::RED));
                        if tech.modifiers.is_empty() {
                            ui.label(
                                egui::RichText::new("This tech grants no modifiers.")
                                    .small()
                                    .italics()
                                    .color(theme::TEXT_DIM),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("Modifiers this tech grants:")
                                    .small()
                                    .color(theme::TEXT),
                            );
                            for m in &tech.modifiers {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "  • {}: {:+.1}%",
                                        m.modifier_type.display_name(),
                                        m.value
                                    ))
                                    .small()
                                    .color(if m.value >= 0.0 {
                                        theme::GREEN
                                    } else {
                                        theme::RED
                                    }),
                                );
                            }
                            if ui
                                .button("⚡ Grant Tech Bonuses")
                                .on_hover_text(
                                    "Instantly apply all modifiers from this technology as debug overrides",
                                )
                                .clicked()
                            {
                                for m in &tech.modifiers {
                                    *debug_settings
                                        .debug_modifiers
                                        .entry(m.modifier_type.clone())
                                        .or_insert(0.0) += m.value;
                                }
                            }
                        }
                        ui.add_space(3.0);
                        if ui
                            .button("➕ Custom Modifier…")
                            .on_hover_text("Open the Add Debug Modifier dialog")
                            .clicked()
                        {
                            debug_settings.modifier_dialog_show = true;
                        }
                    }
                });
        }
    }

    // Persist selection
    ui.data_mut(|data| {
        if let Some(ref sel) = selected_tech {
            data.insert_persisted(sel_persist_id, sel.clone());
        } else {
            data.remove::<String>(sel_persist_id);
        }
    });

    // ---------- status bar ----------
    let status_rect = egui::Rect::from_min_max(
        egui::Pos2::new(avail.min.x, avail.max.y - status_height),
        avail.max,
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(status_rect), |ui| {
        ui.horizontal(|ui| {
            ui.label("Status:");
            ui.colored_label(theme::GREEN, "● Unlocked");
            ui.colored_label(theme::RP_BLUE, "● Researching");
            ui.colored_label(theme::AMBER, "● Available");
            ui.colored_label(theme::TEXT_HINT, "● Locked");
            ui.label(format!("| Zoom: {:.1}x", zoom));
            if debug_enabled {
                ui.separator();
                ui.colored_label(theme::RED, "Right-click: edit/add techs");
            }
            ui.separator();
            if let Some(ref sel_id) = selected_tech {
                if let Some(sel_tech) = tech_data.technologies.get(sel_id) {
                    ui.label(egui::RichText::new("Selected:").strong());
                    ui.label(&sel_tech.name);
                    ui.label(format!(
                        "({} prerequisites highlighted)",
                        path_techs.len().saturating_sub(1)
                    ));
                }
            } else {
                ui.label(
                    egui::RichText::new("Click a technology to highlight its prerequisite path")
                        .italics(),
                );
            }
        });
    });
}

/// Render the edit/add technology dialog window
pub(super) fn render_tech_edit_dialog(
    ui: &mut egui::Ui,
    tech_data: &mut TechnologiesData,
    edit_state: &mut TechTreeEditState,
    is_add: bool,
) {
    let data_opt = if is_add {
        &mut edit_state.adding
    } else {
        &mut edit_state.editing
    };

    let title = if is_add {
        "Add New Technology"
    } else {
        "Edit Technology"
    };

    let mut should_save = false;
    let mut should_close = false;

    if let Some(ref mut edit_data) = data_opt {
        let mut open = true;
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(450.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(500.0)
                    .show(ui, |ui| {
                        egui::Grid::new("tech_edit_grid")
                            .num_columns(2)
                            .spacing([10.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                // ID
                                ui.label("ID:");
                                if is_add {
                                    ui.text_edit_singleline(&mut edit_data.id);
                                } else {
                                    ui.label(
                                        egui::RichText::new(&edit_data.id)
                                            .monospace()
                                            .color(theme::TEXT_DIM),
                                    );
                                }
                                ui.end_row();

                                // Name
                                ui.label("Name:");
                                ui.text_edit_singleline(&mut edit_data.name);
                                ui.end_row();

                                // Category
                                ui.label("Category:");
                                let categories = TechCategory::all();
                                egui::ComboBox::from_id_salt("tech_edit_cat")
                                    .selected_text(
                                        categories
                                            .get(edit_data.category_index)
                                            .map(|c| c.display_name())
                                            .unwrap_or("Unknown"),
                                    )
                                    .show_ui(ui, |ui| {
                                        for (i, cat) in categories.iter().enumerate() {
                                            ui.selectable_value(
                                                &mut edit_data.category_index,
                                                i,
                                                cat.display_name(),
                                            );
                                        }
                                    });
                                ui.end_row();

                                // Description
                                ui.label("Description:");
                                ui.text_edit_multiline(&mut edit_data.description);
                                ui.end_row();

                                // Research Cost
                                ui.label("Research Cost:");
                                ui.text_edit_singleline(&mut edit_data.research_cost);
                                ui.end_row();

                                // Tier
                                ui.label("Tier:");
                                ui.text_edit_singleline(&mut edit_data.tier);
                                ui.end_row();
                            });

                        ui.add_space(10.0);

                        // Prerequisites section
                        ui.label(egui::RichText::new("Prerequisites:").strong());
                        ui.group(|ui| {
                            let mut remove_idx: Option<usize> = None;
                            for (i, prereq) in edit_data.prerequisites.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    let exists = tech_data.technologies.contains_key(prereq);
                                    let color = if exists { theme::GREEN } else { theme::RED };
                                    ui.colored_label(color, prereq);
                                    if ui.small_button("✖").clicked() {
                                        remove_idx = Some(i);
                                    }
                                });
                            }
                            if let Some(idx) = remove_idx {
                                edit_data.prerequisites.remove(idx);
                            }

                            // Add prerequisite
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt("add_prereq_combo")
                                    .selected_text(if edit_data.new_prereq.is_empty() {
                                        "Select prerequisite..."
                                    } else {
                                        &edit_data.new_prereq
                                    })
                                    .show_ui(ui, |ui| {
                                        let mut sorted_ids: Vec<_> = tech_data
                                            .technologies
                                            .keys()
                                            .filter(|id| {
                                                !edit_data.prerequisites.contains(id)
                                                    && **id != edit_data.id
                                            })
                                            .cloned()
                                            .collect();
                                        sorted_ids.sort();
                                        for tid in sorted_ids {
                                            let label = tech_data
                                                .technologies
                                                .get(&tid)
                                                .map(|t| format!("{} ({})", t.name, tid))
                                                .unwrap_or_else(|| tid.clone());
                                            ui.selectable_value(
                                                &mut edit_data.new_prereq,
                                                tid,
                                                label,
                                            );
                                        }
                                    });
                                if ui.button("➕ Add").clicked() && !edit_data.new_prereq.is_empty()
                                {
                                    edit_data.prerequisites.push(edit_data.new_prereq.clone());
                                    edit_data.new_prereq.clear();
                                }
                            });
                        });

                        ui.add_space(10.0);

                        // Modifiers section
                        ui.label(
                            egui::RichText::new("Modifiers (granted when researched):").strong(),
                        );
                        ui.group(|ui| {
                            let mut remove_idx: Option<usize> = None;
                            if edit_data.modifiers.is_empty() {
                                ui.label(
                                    egui::RichText::new("No modifiers")
                                        .italics()
                                        .color(theme::TEXT_DIM),
                                );
                            }
                            for (i, m) in edit_data.modifiers.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.colored_label(
                                        if m.value >= 0.0 {
                                            theme::GREEN
                                        } else {
                                            theme::RED
                                        },
                                        format!(
                                            "{}: {:+.1}%",
                                            m.modifier_type.display_name(),
                                            m.value
                                        ),
                                    );
                                    if ui.small_button("✖").clicked() {
                                        remove_idx = Some(i);
                                    }
                                });
                            }
                            if let Some(idx) = remove_idx {
                                edit_data.modifiers.remove(idx);
                            }

                            // Add modifier row
                            ui.horizontal(|ui| {
                                let all_mods = ModifierType::all_for_debug();
                                let selected_name = all_mods
                                    .get(edit_data.new_modifier_type_index)
                                    .map(|m| m.display_name())
                                    .unwrap_or_default();
                                egui::ComboBox::from_id_salt("add_modifier_combo")
                                    .selected_text(selected_name)
                                    .show_ui(ui, |ui| {
                                        for (i, m) in all_mods.iter().enumerate() {
                                            ui.selectable_value(
                                                &mut edit_data.new_modifier_type_index,
                                                i,
                                                m.display_name(),
                                            );
                                        }
                                    });
                                ui.add(
                                    egui::TextEdit::singleline(&mut edit_data.new_modifier_value)
                                        .hint_text("value %")
                                        .desired_width(70.0),
                                );
                                if ui.button("➕ Add").clicked() {
                                    if let Ok(val) =
                                        edit_data.new_modifier_value.trim().parse::<f64>()
                                    {
                                        let mtype =
                                            all_mods[edit_data.new_modifier_type_index].clone();
                                        edit_data.modifiers.push(TechModifierDef {
                                            modifier_type: mtype,
                                            value: val,
                                        });
                                        edit_data.new_modifier_value.clear();
                                    }
                                }
                            });
                        });

                        ui.add_space(10.0);

                        // Validation
                        let mut errors: Vec<String> = Vec::new();
                        if edit_data.id.is_empty() {
                            errors.push("ID is required".to_string());
                        }
                        if edit_data.name.is_empty() {
                            errors.push("Name is required".to_string());
                        }
                        if edit_data.research_cost.parse::<f64>().is_err() {
                            errors.push("Research cost must be a number".to_string());
                        }
                        if edit_data.tier.parse::<u32>().is_err() {
                            errors.push("Tier must be a positive integer".to_string());
                        }
                        if is_add && tech_data.technologies.contains_key(&edit_data.id) {
                            errors.push(format!("ID '{}' already exists", edit_data.id));
                        }

                        if !errors.is_empty() {
                            for err in &errors {
                                ui.colored_label(theme::RED, err);
                            }
                        }

                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            let can_save = errors.is_empty();
                            if ui
                                .add_enabled(can_save, egui::Button::new("💾 Save"))
                                .clicked()
                            {
                                should_save = true;
                            }
                            if ui.button("Cancel").clicked() {
                                should_close = true;
                            }
                        });
                    });
            });
        if !open {
            should_close = true;
        }
    }

    // Apply save outside borrow scope
    if should_save {
        let data_opt = if is_add {
            &mut edit_state.adding
        } else {
            &mut edit_state.editing
        };

        if let Some(edit_data) = data_opt.take() {
            let categories = TechCategory::all();
            let category = categories
                .get(edit_data.category_index)
                .copied()
                .unwrap_or(TechCategory::Physics);
            let research_cost = edit_data.research_cost.parse::<f64>().unwrap_or(1000.0);
            let tier = edit_data.tier.parse::<u32>().unwrap_or(1);

            if !is_add {
                // Editing existing tech — update in place, preserving unlocks/modifiers
                if let Some(tech) = tech_data.technologies.get_mut(&edit_data.original_id) {
                    tech.name = edit_data.name;
                    tech.category = category;
                    tech.description = edit_data.description;
                    tech.research_cost = research_cost;
                    tech.tier = tier;
                    tech.prerequisites = edit_data.prerequisites;
                    tech.modifiers = edit_data.modifiers;
                }
            } else {
                // Adding new tech
                let new_tech = crate::research::types::Technology {
                    id: edit_data.id.clone(),
                    name: edit_data.name,
                    category,
                    description: edit_data.description,
                    research_cost,
                    prerequisites: edit_data.prerequisites,
                    unlocks_components: Vec::new(),
                    unlocks_engineering: Vec::new(),
                    modifiers: edit_data.modifiers,
                    tier,
                };
                tech_data.technologies.insert(edit_data.id, new_tech);
            }
            save_technologies_to_file(tech_data);
        }
    } else if should_close {
        if is_add {
            edit_state.adding = None;
        } else {
            edit_state.editing = None;
        }
    }
}

/// Save the current technologies data back to the RON file
pub(super) fn save_technologies_to_file(tech_data: &TechnologiesData) {
    use serde::Serialize;

    #[derive(Serialize)]
    struct TechnologiesFile {
        technologies: Vec<crate::research::types::Technology>,
        components: Vec<crate::research::types::ComponentDefinition>,
    }

    let mut techs: Vec<_> = tech_data.technologies.values().cloned().collect();
    techs.sort_by(|a, b| {
        a.tier.cmp(&b.tier).then_with(|| {
            a.category
                .cmp(&b.category)
                .then_with(|| a.name.cmp(&b.name))
        })
    });

    let mut comps: Vec<_> = tech_data.components.values().cloned().collect();
    comps.sort_by(|a, b| a.id.cmp(&b.id));

    let file_data = TechnologiesFile {
        technologies: techs,
        components: comps,
    };

    let pretty_config = ron::ser::PrettyConfig::new()
        .depth_limit(4)
        .struct_names(false)
        .enumerate_arrays(false);

    match ron::ser::to_string_pretty(&file_data, pretty_config) {
        Ok(contents) => {
            let path = "assets/data/technologies.ron";
            match std::fs::write(path, &contents) {
                Ok(()) => info!("Saved technologies to {}", path),
                Err(e) => error!("Failed to write technologies file: {}", e),
            }
        }
        Err(e) => error!("Failed to serialize technologies: {}", e),
    }
}

/// Get the unique category color for a TechCategory. Delegates to
/// `theme::tech_category_color` so the palette is defined in one place.
pub(super) fn tech_category_color(cat: TechCategory) -> egui::Color32 {
    theme::tech_category_color(cat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn pos(x: f32, y: f32) -> egui::Pos2 {
        egui::Pos2::new(x, y)
    }

    fn map_of(positions: &[(&str, egui::Pos2)]) -> HashMap<String, egui::Pos2> {
        positions
            .iter()
            .map(|(id, p)| (id.to_string(), *p))
            .collect()
    }

    #[test]
    fn nav_empty_returns_none() {
        let map = HashMap::new();
        assert!(nearest_tech_in_direction(&map, None, TechNavDirection::Right).is_none());
    }

    #[test]
    fn nav_no_selection_picks_top_left() {
        let map = map_of(&[
            ("a", pos(50.0, 200.0)),
            ("b", pos(20.0, 30.0)),
            ("c", pos(200.0, 5.0)),
        ]);
        // Top-left = smallest y, tiebreaker = smallest x → "c" (y=5) before "b" (y=30).
        assert_eq!(
            nearest_tech_in_direction(&map, None, TechNavDirection::Right),
            Some("c".to_string())
        );
    }

    #[test]
    fn nav_right_picks_closest_in_x() {
        let map = map_of(&[
            ("start", pos(0.0, 0.0)),
            ("right_far", pos(300.0, 5.0)),
            ("right_near", pos(80.0, 50.0)),
        ]);
        // "right_near" wins: smaller primary (80 < 300), and the secondary
        // penalty (|dy|=50) only adds 25 to the score vs |dy|=5 adding 2.5.
        // Score(near) = 80 + 0.5*50 = 105; Score(far) = 300 + 0.5*5 = 302.5.
        assert_eq!(
            nearest_tech_in_direction(&map, Some("start"), TechNavDirection::Right),
            Some("right_near".to_string())
        );
    }

    #[test]
    fn nav_left_excludes_right_node() {
        let map = map_of(&[
            ("start", pos(0.0, 0.0)),
            ("to_the_right", pos(100.0, 0.0)),
            ("to_the_left", pos(-50.0, 0.0)),
        ]);
        // Right of start is excluded by direction filter.
        assert_eq!(
            nearest_tech_in_direction(&map, Some("start"), TechNavDirection::Left),
            Some("to_the_left".to_string())
        );
    }

    #[test]
    fn nav_returns_none_when_no_neighbour_in_direction() {
        let map = map_of(&[("start", pos(0.0, 0.0)), ("only", pos(10.0, 0.0))]);
        // Asking Left from "start" → no node to the left.
        assert_eq!(
            nearest_tech_in_direction(&map, Some("start"), TechNavDirection::Left),
            None
        );
    }

    #[test]
    fn nav_down_picks_closest_in_y() {
        let map = map_of(&[
            ("start", pos(0.0, 0.0)),
            ("far_below", pos(5.0, 400.0)),
            ("near_below", pos(50.0, 80.0)),
        ]);
        // Score(near) = 80 + 0.5*50 = 105; Score(far) = 400 + 0.5*5 = 402.5.
        assert_eq!(
            nearest_tech_in_direction(&map, Some("start"), TechNavDirection::Down),
            Some("near_below".to_string())
        );
    }

    #[test]
    fn nav_up_picks_closest_in_y_above() {
        let map = map_of(&[
            ("start", pos(0.0, 100.0)),
            ("just_above", pos(50.0, 20.0)),
            ("way_above", pos(5.0, -300.0)),
        ]);
        assert_eq!(
            nearest_tech_in_direction(&map, Some("start"), TechNavDirection::Up),
            Some("just_above".to_string())
        );
    }

    #[test]
    fn nav_stale_current_falls_back_to_top_left() {
        let map = map_of(&[("a", pos(0.0, 0.0)), ("b", pos(10.0, 10.0))]);
        // current id doesn't exist → should still return a valid id.
        let next = nearest_tech_in_direction(&map, Some("ghost"), TechNavDirection::Right);
        assert_eq!(next, Some("a".to_string()));
    }

    #[test]
    fn nav_key_from_arrow_keys() {
        assert_eq!(
            TechNavDirection::from_key(egui::Key::ArrowUp),
            Some(TechNavDirection::Up)
        );
        assert_eq!(
            TechNavDirection::from_key(egui::Key::ArrowDown),
            Some(TechNavDirection::Down)
        );
        assert_eq!(
            TechNavDirection::from_key(egui::Key::ArrowLeft),
            Some(TechNavDirection::Left)
        );
        assert_eq!(
            TechNavDirection::from_key(egui::Key::ArrowRight),
            Some(TechNavDirection::Right)
        );
        assert_eq!(TechNavDirection::from_key(egui::Key::Enter), None);
        assert_eq!(TechNavDirection::from_key(egui::Key::Space), None);
    }
}
