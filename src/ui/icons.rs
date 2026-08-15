use super::*;

/// Converts dark-on-light UI artwork into a clean, tintable white icon.
///
/// A narrow luminance threshold removes pale background noise while keeping
/// the main line art solid. The result is premultiplied white, matching the
/// egui texture pipeline used by the resource and building icons.
pub(super) fn process_line_art_pixel(chunk: &mut [u8]) {
    let r = chunk[0] as f32 / 255.0;
    let g = chunk[1] as f32 / 255.0;
    let b = chunk[2] as f32 / 255.0;
    let luminance = 0.299 * r + 0.587 * g + 0.114 * b;

    // Preserve strong lines, discard faint scan/texture noise, and retain a
    // small antialiased edge instead of using a cubic curve that exaggerates
    // gray speckling when the icon is rendered at 14–20 px.
    let alpha = ((0.86 - luminance) / (0.86 - 0.42)).clamp(0.0, 1.0);
    let pa = (alpha * 255.0).round() as u8;
    chunk[0] = pa;
    chunk[1] = pa;
    chunk[2] = pa;
    chunk[3] = pa;
}

/// Loaded textures for the top menu icons
#[derive(Resource, Default)]
pub struct MenuIcons {
    pub handles: HashMap<GameMenu, Handle<Image>>,
    /// Menus that have already been post-processed (white -> transparent)
    pub processed: std::collections::HashSet<GameMenu>,
}

/// Startup system to load menu icon images from assets/textures/ui/menu/
pub(super) fn load_menu_icons(mut commands: Commands, asset_server: Res<AssetServer>) {
    let mut map = HashMap::new();
    for &menu in GameMenu::all() {
        // File names follow the game's convention, e.g. "main.png", "starmap.png"
        let filename = format!("textures/ui/menu/{}.png", menu.asset_basename());
        let handle: Handle<Image> = asset_server.load(&filename);
        map.insert(menu, handle);
    }
    commands.insert_resource(MenuIcons {
        handles: map,
        processed: Default::default(),
    });
}

/// Post-process loaded icon images:
/// 1. Calculate alpha from luminance (inverted) to remove white background
/// 2. Set all RGB pixels to WHITE so they can be tinted at runtime
pub(super) fn process_menu_icons(
    mut menu_icons: ResMut<MenuIcons>,
    mut images: ResMut<Assets<Image>>,
) {
    // Collect handles to process to avoid mutable/immutable borrow conflicts
    let to_process: Vec<(GameMenu, Handle<Image>)> = menu_icons
        .handles
        .iter()
        .filter(|(menu, _)| !menu_icons.processed.contains(menu))
        .map(|(m, h)| (*m, h.clone()))
        .collect();

    for (menu, handle) in to_process {
        if let Some(image) = images.get_mut(&handle) {
            // Only handle 4-byte-per-pixel formats (assume RGBA8)
            let bytes_per_pixel = 4usize;
            if image.data.as_ref().unwrap().len()
                != (image.texture_descriptor.size.width as usize)
                    .saturating_mul(image.texture_descriptor.size.height as usize)
                    .saturating_mul(bytes_per_pixel)
            {
                // Unsupported format, mark processed to avoid retrying
                menu_icons.processed.insert(menu);
                continue;
            }

            for chunk in image
                .data
                .as_mut()
                .unwrap()
                .chunks_exact_mut(bytes_per_pixel)
            {
                process_line_art_pixel(chunk);
            }

            // Mark as processed so we only do this once per asset
            menu_icons.processed.insert(menu);
        }
    }
}

/// Loaded textures for research category icons
#[derive(Resource, Default)]
pub struct ResearchIcons {
    pub handles: HashMap<TechCategory, Handle<Image>>,
    /// Icons that have already been post-processed
    pub processed: std::collections::HashSet<TechCategory>,
}

/// Startup system to load research icons from assets/textures/ui/research/
pub(super) fn load_research_icons(mut commands: Commands, asset_server: Res<AssetServer>) {
    let mut map = HashMap::new();
    for &category in TechCategory::all() {
        let name = match category {
            TechCategory::Electronics => "electronics",
            TechCategory::Military => "military",
            TechCategory::SpaceTechnology => "space_technology",
            TechCategory::Biology => "biology",
            TechCategory::Physics => "physics",
            TechCategory::Energy => "energy",
            TechCategory::Sociology => "sociology",
            TechCategory::Construction => "construction",
            TechCategory::Propulsion => "propulsion",
            TechCategory::Materials => "materials",
            TechCategory::Sensors => "sensors",
            TechCategory::Weapons => "weapons",
            TechCategory::DefensiveSystems => "defensive_systems",
            TechCategory::LifeSupport => "life_support",
            TechCategory::Industry => "industry",
        };
        // Expected path: assets/textures/ui/research/{category}.png
        let filename = format!("textures/ui/research/{}.png", name);
        let handle: Handle<Image> = asset_server.load(&filename);
        map.insert(category, handle);
    }
    commands.insert_resource(ResearchIcons {
        handles: map,
        processed: Default::default(),
    });
}

/// Post-process loaded research icon images (same as menu icons)
pub(super) fn process_research_icons(
    mut icons: ResMut<ResearchIcons>,
    mut images: ResMut<Assets<Image>>,
) {
    // Collect handles to process
    let to_process: Vec<(TechCategory, Handle<Image>)> = icons
        .handles
        .iter()
        .filter(|(cat, _)| !icons.processed.contains(cat))
        .map(|(c, h)| (*c, h.clone()))
        .collect();

    for (category, handle) in to_process {
        if let Some(image) = images.get_mut(&handle) {
            let bytes_per_pixel = 4usize;
            if image.data.as_ref().unwrap().len()
                != (image.texture_descriptor.size.width as usize)
                    .saturating_mul(image.texture_descriptor.size.height as usize)
                    .saturating_mul(bytes_per_pixel)
            {
                icons.processed.insert(category);
                continue;
            }

            for chunk in image
                .data
                .as_mut()
                .unwrap()
                .chunks_exact_mut(bytes_per_pixel)
            {
                process_line_art_pixel(chunk);
            }

            icons.processed.insert(category);
        }
    }
}
