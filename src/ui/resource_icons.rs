//! Resource icon loading for the egui-based resources bar AND the
//! bevy_ui construction canary.
//!
//! The building + mining cards (bevy_ui canary) draw their resource
//! demands with `ImageNode` and need `Handle<Image>`s; the legacy
//! `resources_bar.rs` panel is still rendered with **egui** (not
//! bevy_ui) and wants `egui::TextureHandle`s. Both pipelines share
//! the same on-disk PNGs (`assets/textures/ui/resources/<name>.png`)
//! and the same post-processing recipe (white background →
//! transparent, dark lines → premultiplied white) so the icons look
//! identical wherever they appear.
//!
//! There are two loaders:
//!
//! * `load_resource_icons` (egui path) — decodes the PNGs via the
//!   `image` crate on every frame and registers the result with the
//!   egui context. Stores `egui::TextureHandle`s in
//!   `ResourceIcons::handles`.
//! * `load_resource_icons_bevy_ui` (bevy_ui path) — runs once at
//!   `Startup` and asks Bevy's `AssetServer` for each PNG, then a
//!   post-processor mutates the loaded `Image` assets in place
//!   (white → transparent, dark → premultiplied white). Stores
//!   `Handle<Image>`s in `ResourceIcons::bevy_handles`.
//!
//! `post_process_resource_icons` runs every frame in `Update` until
//! every entry has been mutated; once marked processed, the entry
//! stays processed. This mirrors the `MenuIcons` / `ResearchIcons`
//! pattern from `src/ui/icons.rs`.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_egui::egui;

use crate::economy::ResourceType;

/// Loaded textures for the resources-bar icons, keyed by
/// `ResourceType`. Two parallel maps live here:
/// * `handles` — egui `TextureHandle`s consumed by the legacy
///   `resources_bar.rs` panel (kept for backward compatibility;
///   populated by `load_resource_icons`).
/// * `bevy_handles` — Bevy `Handle<Image>`s consumed by the
///   construction canary's `ImageNode` row. Populated by
///   `load_resource_icons_bevy_ui` + `post_process_resource_icons`.
///
/// Plus a `category_handles` map for the 9 category-badge PNGs
/// (`assets/textures/ui/resources/category-*.png`) that the top
/// resource bar shows on each category tile. Category icons are
/// NOT keyed by `ResourceType` (the category is a label, not a
/// resource), so they live in a separate `String`-keyed map.
#[derive(Resource, Default)]
pub struct ResourceIcons {
    /// One `egui::TextureHandle` per resource. Missing entries
    /// (e.g. an icon file wasn't authored yet) leave the key
    /// absent; the render side falls back to a small cyan
    /// placeholder square in that case.
    pub handles: HashMap<ResourceType, egui::TextureHandle>,
    /// Bevy `Handle<Image>` per resource for the bevy_ui canary
    /// (build-card resource demands). The same post-processing
    /// (white → transparent, dark → premultiplied white) is
    /// applied so a tinted `ImageNode` blends cleanly onto the
    /// dark navy card. Missing entries fall back to a small
    /// tinted placeholder square.
    pub bevy_handles: HashMap<ResourceType, Handle<Image>>,
    /// Resources whose PNG has been requested via `AssetServer`
    /// but the asset hasn't finished loading yet. Cleared once
    /// `Assets<Image>::get_mut` returns the decoded buffer and
    /// the post-processor runs. Stays populated for the first
    /// frame or two of `Startup`.
    pub bevy_pending: std::collections::HashSet<ResourceType>,
    /// egui `TextureHandle`s for the 9 category-badge PNGs.
    /// Keyed by the category name exactly as
    /// `ResourceType::by_category()` returns it (`"Atmospheric
    /// Gases"`, `"Fusion Fuel"`, `"Precious Metals"`, etc.). The
    /// on-disk basenames live in `category_icon_basename()`
    /// below. Missing entries fall back to a small tinted
    /// square, the same fallback as resource icons.
    pub category_handles: HashMap<String, egui::TextureHandle>,
}

/// Canonical on-disk basename for each category-badge PNG in
/// `assets/textures/ui/resources/`. Returns `None` for unknown
/// categories so the render side can skip the disk read and fall
/// straight to the placeholder square.
///
/// The slugs mirror the author's file-naming convention:
/// `"Atmospheric Gases"` → `category-atmospheric`, `"Fusion Fuel"`
/// → `category-fusion-fuel`, `"Precious Metals"` →
/// `category-precious`. If a new category is added to
/// `ResourceType::by_category()` without a matching slug here
/// the badge silently falls back to a placeholder; this is the
/// expected dev-mode behaviour.
pub fn category_icon_basename(category: &str) -> Option<&'static str> {
    match category {
        "Biological" => Some("category-biological"),
        "Volatiles" => Some("category-volatiles"),
        "Atmospheric Gases" => Some("category-atmospheric"),
        "Construction" => Some("category-construction"),
        "Fusion Fuel" => Some("category-fusion-fuel"),
        "Fissiles" => Some("category-fissiles"),
        "Precious Metals" => Some("category-precious"),
        "Strategic" => Some("category-strategic"),
        "Exotic" => Some("category-exotic"),
        _ => None,
    }
}

/// Convenience: returns the egui `TextureHandle` for a category
/// badge, or `None` if it hasn't loaded yet. The render side
/// falls back to a tinted placeholder square in that case
/// (matches the resource-icon fallback so the bar still reads as
/// a row of evenly-sized tiles).
pub fn get_category_icon_handle<'a>(
    icons: &'a ResourceIcons,
    category: &str,
) -> Option<&'a egui::TextureHandle> {
    icons.category_handles.get(category)
}

/// One-shot `Startup` loader: asks Bevy's `AssetServer` for each
/// resource icon PNG and stores the resulting `Handle<Image>` in
/// `ResourceIcons::bevy_handles`. The post-processor mutates the
/// loaded pixels in place (see `post_process_resource_icons`).
pub fn load_resource_icons_bevy_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let mut handles = HashMap::new();
    let mut pending = std::collections::HashSet::new();
    for &resource in ResourceType::all() {
        let Some(basename) = resource_bevy_ui_basename(resource) else {
            continue;
        };
        let path = format!("textures/ui/resources/{}.png", basename);
        let handle: Handle<Image> = asset_server.load(&path);
        pending.insert(resource);
        handles.insert(resource, handle);
    }
    commands.insert_resource(ResourceIcons {
        handles: HashMap::new(), // egui path remains untouched
        bevy_handles: handles,
        bevy_pending: pending,
        category_handles: HashMap::new(), // populated by load_resource_icons (egui path)
    });
}

/// On-disk basename for the bevy_ui resource icon path. Mirrors
/// the egui-side `resource_icon_path()` so the two paths load the
/// same file. Kept separate so a future divergence (egui wants
/// rasterised, bevy_ui wants SVG → rasterised) doesn't have to
/// touch both call sites.
fn resource_bevy_ui_basename(resource: ResourceType) -> Option<&'static str> {
    match resource {
        ResourceType::Water => Some("water"),
        ResourceType::Hydrogen => Some("hydrogen"),
        ResourceType::Ammonia => Some("ammonia"),
        ResourceType::Methane => Some("methane"),
        ResourceType::Phosphorus => Some("phosphorus"),
        ResourceType::Food => Some("food"),
        ResourceType::Nitrogen => Some("nitrogen"),
        ResourceType::Oxygen => Some("oxygen"),
        ResourceType::CarbonDioxide => Some("carbon-dioxide"),
        ResourceType::Argon => Some("argon"),
        ResourceType::Iron => Some("iron"),
        ResourceType::Aluminum => Some("aluminum"),
        ResourceType::Titanium => Some("titanium"),
        ResourceType::Silicates => Some("silicates"),
        ResourceType::Nickel => Some("nickel"),
        ResourceType::Tungsten => Some("tungsten"),
        ResourceType::Carbon => Some("carbon"),
        ResourceType::Chromium => Some("chromium"),
        ResourceType::Magnesium => Some("magnesium"),
        ResourceType::Helium3 => Some("helium-3"),
        ResourceType::Deuterium => Some("deuterium"),
        ResourceType::Tritium => Some("tritium"),
        ResourceType::Uranium => Some("uranium"),
        ResourceType::Thorium => Some("thorium"),
        ResourceType::Plutonium => Some("plutonium"),
        ResourceType::Gold => Some("gold"),
        ResourceType::Silver => Some("silver"),
        ResourceType::Platinum => Some("platinum"),
        ResourceType::Copper => Some("copper"),
        ResourceType::RareEarths => Some("rare-earths"),
        ResourceType::Lithium => Some("lithium"),
        ResourceType::Sulfur => Some("sulfur"),
        ResourceType::Cobalt => Some("cobalt"),
        ResourceType::Fluorine => Some("fluorine"),
        ResourceType::Polymers => Some("polymers"),
        ResourceType::Antimatter => Some("antimatter"),
        ResourceType::ExoticMatter => Some("exotic-matter"),
        ResourceType::Metamaterials => Some("metamaterials"),
        ResourceType::Computronium => Some("computronium"),
    }
}

/// Post-processes every bevy_ui resource icon. Runs in `Update`
/// until every entry has been processed, then early-returns. The
/// recipe is the same as `process_menu_icons` and
/// `process_research_icons` from `src/ui/icons.rs`: dark lines on
/// white background → premultiplied white on transparent so the
/// tinted `ImageNode` reads as the resource's category color on
/// the card.
pub fn post_process_resource_icons(
    mut icons: ResMut<ResourceIcons>,
    mut images: ResMut<Assets<Image>>,
) {
    if icons.bevy_pending.is_empty() {
        return;
    }
    // Snapshot the (resource, handle) pairs we still need to
    // process — avoids borrow conflicts on the `icons` map while
    // we mutate `Assets<Image>`.
    let candidates: Vec<(ResourceType, Handle<Image>)> = icons
        .bevy_handles
        .iter()
        .filter(|(r, _)| icons.bevy_pending.contains(r))
        .map(|(r, h)| (*r, h.clone()))
        .collect();

    let bytes_per_pixel = 4usize;
    for (resource, handle) in candidates {
        let Some(image) = images.get_mut(&handle) else {
            // AssetServer hasn't decoded the PNG yet; try again
            // next frame. Skipping is the right behaviour — we
            // can't mutate a buffer we don't have, and forcing
            // the load would block the schedule.
            continue;
        };
        let expected_len = (image.texture_descriptor.size.width as usize)
            .saturating_mul(image.texture_descriptor.size.height as usize)
            .saturating_mul(bytes_per_pixel);
        if image.data.as_ref().unwrap().len() != expected_len {
            // Wrong pixel format (compressed, sRGB-float, …) —
            // mark processed to avoid retrying every frame.
            icons.bevy_pending.remove(&resource);
            continue;
        }
        // Icons are pre-baked (see scripts/bake_resource_icons.py):
        //   - RGB is pure black (0,0,0) for the line
        //   - Alpha is the line opacity: 0 on the white
        //     background, 255 on the solid line, linear
        //     ramp on antialiased edges.
        // All we need to do at runtime is convert RGB to
        // pure white so the tint shader (ImageNode::with_color
        // / egui Image::tint) colours the line; alpha is
        // already correct.
        for chunk in image
            .data
            .as_mut()
            .unwrap()
            .chunks_exact_mut(bytes_per_pixel)
        {
            chunk[0] = 255;
            chunk[1] = 255;
            chunk[2] = 255;
        }
        icons.bevy_pending.remove(&resource);
    }
}

/// Convenience: returns the bevy_ui `Handle<Image>` for a resource,
/// or `None` if it hasn't loaded yet. The render side falls back
/// to a tinted placeholder square in that case (matches the egui
/// fallback).
pub fn get_resource_icon_handle_bevy(
    icons: &ResourceIcons,
    resource: ResourceType,
) -> Option<&Handle<Image>> {
    icons.bevy_handles.get(&resource)
}

/// Per-frame loader: walks the resource list and tries to load
/// each PNG. The image crate decodes PNG synchronously (the icons
/// are 1-700 KB each, the load is fast enough to block the first
/// frame without being noticeable). On success the post-processed
/// `egui::ColorImage` is registered with the egui context and the
/// `TextureHandle` stored in `ResourceIcons`.
///
/// Runs every frame in `Update` so an icon that failed to load on
/// frame N (e.g. because the file was being written) gets a second
/// chance on frame N+1. The `handles` map is the source of truth —
/// once a resource's `TextureHandle` is in the map, we skip the
/// disk read for that resource on every subsequent frame.
pub fn load_resource_icons(
    mut contexts: bevy_egui::EguiContexts,
    mut icons: ResMut<ResourceIcons>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let ctx = ctx.clone();

    for &resource in ResourceType::all() {
        if icons.handles.contains_key(&resource) {
            continue;
        }
        let Some(path) = resource_icon_path(resource) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            // File missing — leave the key absent so the render
            // side falls back to the placeholder. Don't spam the
            // log; the missing-file path is a normal "this
            // resource has no icon yet" state during dev.
            continue;
        };
        let Ok(image) = image::load_from_memory(&bytes) else {
            continue;
        };
        let rgba = image.to_rgba8();
        let (w, h) = rgba.dimensions();
        let processed = post_process_rgba(rgba.as_raw());
        let color_image =
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &processed);
        let handle = ctx.load_texture(
            format!("resource_icon_{:?}", resource),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        icons.handles.insert(resource, handle);
    }

    // Same pattern for the 9 category-badge PNGs
    // (`category-atmospheric.png`, `category-biological.png`, …).
    // The `by_category()` list is the source of truth for which
    // categories exist; `category_icon_basename()` maps each name
    // to its on-disk slug. Categories without a slug (a new
    // category added before the icon is authored) silently fall
    // through to the placeholder.
    for (category, _) in ResourceType::by_category() {
        if icons.category_handles.contains_key(category) {
            continue;
        }
        let Some(slug) = category_icon_basename(category) else {
            continue;
        };
        let path =
            std::path::Path::new("assets/textures/ui/resources").join(format!("{}.png", slug));
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(image) = image::load_from_memory(&bytes) else {
            continue;
        };
        let rgba = image.to_rgba8();
        let (w, h) = rgba.dimensions();
        // Category PNGs are dark-on-WHITE (not pre-baked), so we
        // need the luminance-key recipe to remove the white BG.
        // The regular resource icons go through `post_process_rgba`
        // which assumes the bake already happened.
        let processed = post_process_category_rgba(rgba.as_raw());
        let color_image =
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &processed);
        let handle = ctx.load_texture(
            format!("category_icon_{}", category),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        icons.category_handles.insert(category.to_string(), handle);
    }
}

/// Returns the on-disk path for a resource icon, or `None` if
/// the resource has no icon authored yet. The path is relative to
/// the working directory (bevy + egui both resolve relative to
/// the `assets/` root at startup).
fn resource_icon_path(resource: ResourceType) -> Option<String> {
    let basename = match resource {
        ResourceType::Water => "water",
        ResourceType::Hydrogen => "hydrogen",
        ResourceType::Ammonia => "ammonia",
        ResourceType::Methane => "methane",
        ResourceType::Phosphorus => "phosphorus",
        ResourceType::Food => "food",
        ResourceType::Nitrogen => "nitrogen",
        ResourceType::Oxygen => "oxygen",
        ResourceType::CarbonDioxide => "carbon-dioxide",
        ResourceType::Argon => "argon",
        ResourceType::Iron => "iron",
        ResourceType::Aluminum => "aluminum",
        ResourceType::Titanium => "titanium",
        ResourceType::Silicates => "silicates",
        ResourceType::Nickel => "nickel",
        ResourceType::Tungsten => "tungsten",
        ResourceType::Carbon => "carbon",
        ResourceType::Chromium => "chromium",
        ResourceType::Magnesium => "magnesium",
        ResourceType::Helium3 => "helium-3",
        ResourceType::Deuterium => "deuterium",
        ResourceType::Tritium => "tritium",
        ResourceType::Uranium => "uranium",
        ResourceType::Thorium => "thorium",
        ResourceType::Plutonium => "plutonium",
        ResourceType::Gold => "gold",
        ResourceType::Silver => "silver",
        ResourceType::Platinum => "platinum",
        ResourceType::Copper => "copper",
        ResourceType::RareEarths => "rare-earths",
        ResourceType::Lithium => "lithium",
        ResourceType::Sulfur => "sulfur",
        ResourceType::Cobalt => "cobalt",
        ResourceType::Fluorine => "fluorine",
        ResourceType::Polymers => "polymers",
        ResourceType::Antimatter => "antimatter",
        ResourceType::ExoticMatter => "exotic-matter",
        ResourceType::Metamaterials => "metamaterials",
        ResourceType::Computronium => "computronium",
    };
    // Absolute path is fine here — `std::fs::read` and the
    // `image` crate don't go through Bevy's asset server.
    let abs =
        std::path::Path::new("assets/textures/ui/resources").join(format!("{}.png", basename));
    if !abs.exists() {
        return None;
    }
    Some(abs.to_string_lossy().into_owned())
}

/// Post-process the decoded RGBA pixels for the egui path.
///
/// Icons are pre-baked on disk (see `scripts/bake_resource_icons.py`):
///   - RGB is pure black on the line, fully transparent on the
///     white background.
///   - Alpha is 255 on solid lines, 0 on background, linear
///     ramp on antialiased edges.
///
/// All the runtime does is rewrite RGB to pure white so the
/// egui tint shader (which multiplies white by the tint)
/// produces the final category color. Alpha is passed through
/// unchanged. This is much cheaper than the old
/// luminance-based recipe and immune to JPEG-style noise (the
/// bake already removed the white BG and noise).
fn post_process_rgba(rgba: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; rgba.len()];
    for (src_chunk, dst_chunk) in rgba.chunks_exact(4).zip(out.chunks_exact_mut(4)) {
        dst_chunk[0] = 255;
        dst_chunk[1] = 255;
        dst_chunk[2] = 255;
        dst_chunk[3] = src_chunk[3];
    }
    out
}

/// Apply the shared clean threshold to category artwork while keeping
/// RGB white for egui tinting. The narrow threshold rejects pale paper
/// texture and avoids the noisy cubic falloff used previously.
fn post_process_category_rgba(rgba: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; rgba.len()];
    for (src_chunk, dst_chunk) in rgba.chunks_exact(4).zip(out.chunks_exact_mut(4)) {
        let r = src_chunk[0] as f32 / 255.0;
        let g = src_chunk[1] as f32 / 255.0;
        let b = src_chunk[2] as f32 / 255.0;
        let luminance = 0.299 * r + 0.587 * g + 0.114 * b;
        let alpha = ((0.86 - luminance) / (0.86 - 0.42)).clamp(0.0, 1.0);
        dst_chunk[0] = 255;
        dst_chunk[1] = 255;
        dst_chunk[2] = 255;
        dst_chunk[3] = (alpha * 255.0).round() as u8;
    }
    out
}

/// Convenience: returns the icon handle for a resource, or `None`
/// if it hasn't loaded yet (the render side falls back to a small
/// cyan placeholder in that case).
pub fn get_resource_icon_handle(
    icons: &ResourceIcons,
    resource: ResourceType,
) -> Option<&egui::TextureHandle> {
    icons.handles.get(&resource)
}
