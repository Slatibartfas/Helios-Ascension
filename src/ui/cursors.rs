use bevy::prelude::*;
use bevy::window::{CursorIcon, CustomCursor, CustomCursorImage, PrimaryWindow};
use bevy_egui::{egui, EguiContexts};

/// Handles loading and management of custom cursors.
pub struct CursorPlugin;

impl Plugin for CursorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CursorAssets>()
            .add_systems(Startup, setup_cursors)
            .add_systems(Update, update_cursor_icon);
    }
}

/// Stores handles to cursor images.
#[derive(Resource, Default)]
pub struct CursorAssets {
    pub regular: Handle<Image>,
    pub hover: Handle<Image>,
    pub text: Handle<Image>,
    pub crosshair: Handle<Image>,
}

fn setup_cursors(mut cursor_assets: ResMut<CursorAssets>, asset_server: Res<AssetServer>) {
    cursor_assets.regular = asset_server.load("textures/ui/cursors/regular.png");
    cursor_assets.hover = asset_server.load("textures/ui/cursors/hover.png");
    cursor_assets.text = asset_server.load("textures/ui/cursors/text.png");
    cursor_assets.crosshair = asset_server.load("textures/ui/cursors/crosshair.png");
}

fn update_cursor_icon(
    mut cursor_icon: Query<&mut CursorIcon, With<PrimaryWindow>>,
    cursor_assets: Res<CursorAssets>,
    mut egui_contexts: EguiContexts,
    asset_server: Res<AssetServer>,
    mut last_cursor: Local<Option<egui::CursorIcon>>,
) {
    // Wait until all cursor assets are fully loaded before switching away from
    // the system cursor. This prevents flickering and the brief system-cursor
    // flash while textures stream in during the first few frames.
    let all_loaded = [
        &cursor_assets.regular,
        &cursor_assets.hover,
        &cursor_assets.text,
        &cursor_assets.crosshair,
    ]
    .iter()
    .all(|h| asset_server.is_loaded_with_dependencies(*h));
    if !all_loaded {
        return;
    }

    let ctx = match egui_contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let egui_cursor = ctx.output(|o| o.cursor_icon);

    // Only update the CursorIcon component when the cursor type actually changes.
    // Rebuilding the custom cursor every frame triggers OS-level reload and flicker.
    if *last_cursor == Some(egui_cursor) {
        return;
    }
    *last_cursor = Some(egui_cursor);

    let mut icon_component = match cursor_icon.iter_mut().next() {
        Some(i) => i,
        None => return,
    };

    // Hotspot coordinates are relative to the 32×32 cursor images.
    let (target_handle, hotspot): (&Handle<Image>, (u16, u16)) = match egui_cursor {
        egui::CursorIcon::Default => (&cursor_assets.regular, (2, 2)),
        egui::CursorIcon::PointingHand => (&cursor_assets.hover, (6, 1)),
        egui::CursorIcon::Text => (&cursor_assets.text, (16, 14)),
        egui::CursorIcon::Crosshair => (&cursor_assets.crosshair, (16, 16)),
        egui::CursorIcon::ResizeHorizontal
        | egui::CursorIcon::ResizeVertical
        | egui::CursorIcon::ResizeNeSw
        | egui::CursorIcon::ResizeNwSe => (&cursor_assets.regular, (2, 2)),
        _ => (&cursor_assets.regular, (2, 2)),
    };

    *icon_component = CursorIcon::Custom(CustomCursor::Image(CustomCursorImage {
        handle: target_handle.clone(),
        hotspot,
        ..default()
    }));
}
