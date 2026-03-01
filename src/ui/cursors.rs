use bevy::prelude::*;
use bevy::window::{CursorIcon, CustomCursor, PrimaryWindow, CustomCursorImage};
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
) {
    let mut icon_component = match cursor_icon.iter_mut().next() {
        Some(i) => i,
        None => return,
    };

    let ctx = match egui_contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    
    // Check if we are hovering over any UI widget that requests a specific cursor
    let egui_cursor = ctx.output(|o| o.cursor_icon);

    let (target_handle, hotspot) = match egui_cursor {
        egui::CursorIcon::Default => (&cursor_assets.regular, (2, 2)),
        // Only use hover cursor for active pointing
        egui::CursorIcon::PointingHand => (&cursor_assets.hover, (4, 4)),
        egui::CursorIcon::Text => (&cursor_assets.text, (16, 16)),
        egui::CursorIcon::Crosshair => (&cursor_assets.crosshair, (16, 16)),
        
        // Map other common cursors to regular or context-appropriate ones
        egui::CursorIcon::ResizeHorizontal 
        | egui::CursorIcon::ResizeVertical 
        | egui::CursorIcon::ResizeNeSw 
        | egui::CursorIcon::ResizeNwSe => (&cursor_assets.regular, (8, 8)), // Or create resize cursors later
        
        // Fallback for everything else
        _ => (&cursor_assets.regular, (2, 2)),
    };

    *icon_component = CursorIcon::Custom(CustomCursor::Image(
        CustomCursorImage {
            handle: target_handle.clone(),
            hotspot,
            ..default()
        }
    ));
}
