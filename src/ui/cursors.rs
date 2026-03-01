use bevy::prelude::*;
use bevy::window::{CursorIcon, CustomCursor, CustomCursorImage, PrimaryWindow};
use bevy_egui::{egui, EguiGlobalSettings, EguiOutput, EguiPostUpdateSet};

/// Handles loading and management of custom cursors.
pub struct CursorPlugin;

impl Plugin for CursorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CursorAssets>()
            .add_systems(Startup, (disable_egui_cursor_updates, setup_cursors))
            // Run in PostUpdate *after* bevy_egui has finalized EguiOutput so we
            // read the real cursor type for this frame, not a stale/empty value.
            // Must run before `Last` where bevy_winit applies the cursor.
            .add_systems(
                PostUpdate,
                update_cursor_icon.after(EguiPostUpdateSet::ProcessOutput),
            );
    }
}

/// Disable `bevy_egui`'s built-in cursor management so it doesn't overwrite
/// our `CursorIcon::Custom(...)` with `CursorIcon::System(...)` every frame.
fn disable_egui_cursor_updates(mut egui_settings: ResMut<EguiGlobalSettings>) {
    egui_settings.enable_cursor_icon_updates = false;
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
    mut commands: Commands,
    mut primary_window: Query<(Entity, Option<&mut CursorIcon>), With<PrimaryWindow>>,
    cursor_assets: Res<CursorAssets>,
    // EguiOutput is populated by bevy_egui in EguiPostUpdateSet::ProcessOutput.
    // We run after that set, so this reflects the current frame's cursor request.
    egui_output: Query<&EguiOutput>,
    // Check Assets<Image> directly — this is what bevy_winit reads when applying
    // the custom cursor. If the image is not in Assets<Image> yet, winit silently
    // skips and only retries when CursorIcon changes again — so we gate here.
    images: Res<Assets<Image>>,
    mut last_cursor: Local<Option<egui::CursorIcon>>,
) {
    // Only set CursorIcon::Custom once all images are loaded into Assets<Image>.
    let all_loaded = [
        &cursor_assets.regular,
        &cursor_assets.hover,
        &cursor_assets.text,
        &cursor_assets.crosshair,
    ]
    .iter()
    .all(|h| images.contains(*h));
    if !all_loaded {
        return;
    }

    // Read the cursor type egui requested this frame from the finalized output.
    let egui_cursor = egui_output
        .iter()
        .next()
        .map(|o| o.platform_output.cursor_icon)
        .unwrap_or(egui::CursorIcon::Default);

    // Only update the CursorIcon component when the cursor type actually changes.
    if *last_cursor == Some(egui_cursor) {
        return;
    }

    let (window_entity, existing_icon) = match primary_window.iter_mut().next() {
        Some(v) => v,
        None => return,
    };

    let (target_handle, hotspot): (&Handle<Image>, (u16, u16)) = match egui_cursor {
        egui::CursorIcon::Default => (&cursor_assets.regular, (2, 2)),
        egui::CursorIcon::PointingHand => (&cursor_assets.hover, (16, 16)),
        egui::CursorIcon::Text => (&cursor_assets.text, (16, 16)),
        egui::CursorIcon::Crosshair => (&cursor_assets.crosshair, (16, 16)),
        egui::CursorIcon::ResizeHorizontal
        | egui::CursorIcon::ResizeVertical
        | egui::CursorIcon::ResizeNeSw
        | egui::CursorIcon::ResizeNwSe => (&cursor_assets.regular, (2, 2)),
        _ => (&cursor_assets.regular, (2, 2)),
    };

    let target_icon = CursorIcon::Custom(CustomCursor::Image(CustomCursorImage {
        handle: target_handle.clone(),
        hotspot,
        ..default()
    }));

    if let Some(mut icon_component) = existing_icon {
        *icon_component = target_icon;
    } else {
        commands.entity(window_entity).insert(target_icon);
    }

    *last_cursor = Some(egui_cursor);
}
