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
use bevy::render::render_resource::Extent3d;
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
    /// egui `TextureHandle` for the dedicated energy icon
    /// (`assets/textures/ui/resources/energy.png`). Energy is not
    /// a `ResourceType` (it's a power-balance concept, not a
    /// stockpile), so it gets its own slot instead of living in
    /// `handles`. Tinted at the call site: green for surplus,
    /// red for deficit on the top resource bar's power chip;
    /// cyan by default on the forecast popup and build cards.
    /// `None` until `load_resource_icons` decodes the PNG; the
    /// render side falls back to a tinted square in that case.
    pub energy_handle: Option<egui::TextureHandle>,
    /// Bevy `Handle<Image>` for the energy icon, consumed by the
    /// bevy_ui canary (Build/Mining card energy demand + production
    /// rows). Same post-processing as the per-resource bevy handles.
    pub energy_bevy_handle: Option<Handle<Image>>,
    /// Set to `true` while the bevy_ui energy icon is waiting on
    /// the asset server to finish decoding. Cleared once
    /// `Assets<Image>::get_mut` returns the buffer and the
    /// post-processor runs.
    pub energy_bevy_pending: bool,
    /// Edge length (px) the egui textures in `handles`,
    /// `category_handles` and `energy_handle` were baked at. Icons are
    /// resampled to roughly their display size at load time (see
    /// `downscale_icon_rgba`), so a DPI change makes the cached
    /// textures the wrong resolution. `load_resource_icons` drops the
    /// egui caches and rebakes when the computed size changes.
    ///
    /// Keyed on the *texture size* rather than `pixels_per_point`
    /// directly: the size is a clamped integer, so a jittering scale
    /// factor can't thrash 48 PNG decodes every frame. `0` on startup,
    /// which never matches a real size, so the first frame always bakes.
    pub texture_size: u32,
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
    // Energy is not a ResourceType (it's a power concept), so it
    // lives in its own slot. Load it through the asset server so
    // the bevy_ui canary (Build / Mining cards) can tint the
    // `ImageNode` per-row (red for demand, green for production).
    let energy_bevy_handle: Handle<Image> =
        asset_server.load("textures/ui/resources/energy.png");
    commands.insert_resource(ResourceIcons {
        handles: HashMap::new(), // egui path remains untouched
        bevy_handles: handles,
        bevy_pending: pending,
        category_handles: HashMap::new(), // populated by load_resource_icons (egui path)
        energy_handle: None,              // populated by load_resource_icons (egui path)
        energy_bevy_handle: Some(energy_bevy_handle),
        energy_bevy_pending: true,
        texture_size: 0, // forces the egui loader to bake on its first frame
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
    if icons.bevy_pending.is_empty() && !icons.energy_bevy_pending {
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

    for (resource, handle) in candidates {
        let Some(image) = images.get_mut(&handle) else {
            // AssetServer hasn't decoded the PNG yet; try again
            // next frame. Skipping is the right behaviour — we
            // can't mutate a buffer we don't have, and forcing
            // the load would block the schedule.
            continue;
        };
        // Icons are pre-baked (see scripts/bake_resource_icons.py):
        //   - RGB is pure black (0,0,0) for the line
        //   - Alpha is the line opacity: 0 on the white
        //     background, 255 on the solid line, linear
        //     ramp on antialiased edges.
        // The runtime converts RGB to pure white so the tint shader
        // (ImageNode::with_color / egui Image::tint) colours the line,
        // then resamples 1024 px → 64 px so the 20 px card chip isn't
        // minified 51:1 by a mip-less bilinear sampler.
        //
        // A `false` return means the wrong pixel format (compressed,
        // sRGB-float, …) — mark processed to avoid retrying every frame.
        process_and_downscale_bevy_icon(image);
        icons.bevy_pending.remove(&resource);
    }

    // Dedicated energy icon. v0.5.2 PR-A.7 (2026-08-04): unlike
    // the per-resource icons (which are pre-baked on disk to
    // RGB-black + alpha-keyed), the energy PNG is a fresh
    // `image_synthesize` output with the standard white
    // background — alpha is 255 everywhere. Apply the
    // luminance-key recipe (`post_process_category_rgba` on
    // the egui side) IN PLACE before the standard RGB→white +
    // downscale so the bolt-in-hex PNG ends up with a
    // transparent background and a white tintable line. Without
    // this step the icon renders as a solid coloured square
    // (the entire 1024×1024 image is opaque, the tint fills
    // every pixel, the line is invisible against the
    // background).
    if icons.energy_bevy_pending {
        let Some(handle) = icons.energy_bevy_handle.clone() else {
            icons.energy_bevy_pending = false;
            return;
        };
        let Some(image) = images.get_mut(&handle) else {
            // AssetServer hasn't decoded yet; try again next frame.
            return;
        };
        // Step 1: apply the luminance key in place — rewrite
        // alpha based on the source RGB luminance, keep RGB
        // untouched for now.
        if let Some(data) = image.data.as_mut() {
            let w = image.texture_descriptor.size.width;
            let h = image.texture_descriptor.size.height;
            if w > 0 && h > 0 && data.len() == (w as usize).saturating_mul(h as usize) * 4 {
                for chunk in data.chunks_exact_mut(4) {
                    let r = chunk[0] as f32 / 255.0;
                    let g = chunk[1] as f32 / 255.0;
                    let b = chunk[2] as f32 / 255.0;
                    let luminance = 0.299 * r + 0.587 * g + 0.114 * b;
                    let alpha = ((0.86 - luminance) / (0.86 - 0.42)).clamp(0.0, 1.0);
                    chunk[3] = (alpha * 255.0).round() as u8;
                }
            }
        }
        // Step 2: standard RGB→white + downscale. The alpha is
        // already keyed so the line survives as the only opaque
        // pixel after the downsample.
        process_and_downscale_bevy_icon(image);
        icons.energy_bevy_pending = false;
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

    // Icons are baked at (roughly) their display size rather than the
    // 1024 px source resolution — see `downscale_icon_rgba`. That makes
    // the textures DPI-dependent, so when the window moves to a monitor
    // with a different scale factor the cached textures are the wrong
    // resolution and have to be rebuilt.
    let target = icon_texture_size(ctx.pixels_per_point());
    if icons.texture_size != target {
        icons.handles.clear();
        icons.category_handles.clear();
        icons.energy_handle = None;
        icons.texture_size = target;
    }

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
        let (processed, w, h) = downscale_icon_rgba(&processed, w, h, target);
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
        let (processed, w, h) = downscale_icon_rgba(&processed, w, h, target);
        let color_image =
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &processed);
        let handle = ctx.load_texture(
            format!("category_icon_{}", category),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        icons.category_handles.insert(category.to_string(), handle);
    }

    // Dedicated energy icon (`assets/textures/ui/resources/energy.png`).
    // Energy is not a `ResourceType` — it's the power-balance concept
    // used by the top resource bar's "TW" chip and (eventually) the
    // Build/Mining card energy rows. Loaded with the same
    // luminance-key recipe as the category badges (dark navy on white
    // → premultiplied white on transparent) and tinted at the call
    // site: green/red for the power chip, cyan by default elsewhere.
    if icons.energy_handle.is_none() {
        let path = std::path::Path::new("assets/textures/ui/resources/energy.png");
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(image) = image::load_from_memory(&bytes) {
                let rgba = image.to_rgba8();
                let (w, h) = rgba.dimensions();
                let processed = post_process_category_rgba(rgba.as_raw());
                let (processed, w, h) = downscale_icon_rgba(&processed, w, h, target);
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    &processed,
                );
                let handle = ctx.load_texture(
                    "energy_icon".to_string(),
                    color_image,
                    egui::TextureOptions::LINEAR,
                );
                icons.energy_handle = Some(handle);
            }
        }
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

/// Largest logical (pre-DPI) size any egui call site asks an icon to
/// draw at: category badges 30 pt (`resources_bar.rs`, category popup
/// header), resource icons 24 pt (forecast window header), energy 16 pt
/// (power chip). Rounded up to 32 so one baked texture serves all three.
const ICON_MAX_LOGICAL_SIZE: f32 = 32.0;

/// Bounds on the baked texture edge. The floor keeps a usable texture if
/// `pixels_per_point` is reported as ~0 while the window is initialising;
/// the ceiling stops a 4× HiDPI display from baking the full 1024 px
/// source straight back in and re-creating the problem.
const ICON_TEXTURE_MIN: u32 = 32;
const ICON_TEXTURE_MAX: u32 = 256;

/// Baked texture edge length for the current DPI.
fn icon_texture_size(pixels_per_point: f32) -> u32 {
    let px = ICON_MAX_LOGICAL_SIZE * pixels_per_point;
    if !px.is_finite() {
        return ICON_TEXTURE_MIN;
    }
    (px.ceil() as u32).clamp(ICON_TEXTURE_MIN, ICON_TEXTURE_MAX)
}

/// Downscales an already-post-processed RGBA buffer to `target` px on its
/// long edge, using an area-integrating filter.
///
/// This is the fix for the top-bar icons looking crunchy. The source PNGs
/// are 1024×1024 and the top bar draws them at 28 pt, but nothing resized
/// them — the full 1024 px texture went to the GPU and the sampler
/// minified it 36:1 with a 2×2 bilinear tap and no mip chain, i.e. it
/// averaged 4 of the ~1340 source texels covering each output pixel
/// (0.3% of the footprint). That is undersampling, not filtering: thin
/// strokes drop out where they fall between taps and crawl under motion.
///
/// Both post-processors leave RGB at a constant 255 and carry all the
/// shape in alpha, so only alpha needs resampling — and because the
/// colour is constant, alpha *is* the premultiplied value. That sidesteps
/// the usual premultiply-before-you-filter trap: there is no colour to
/// bleed in from transparent texels, and no gamma decision to get wrong,
/// since alpha is linear coverage by definition rather than an
/// sRGB-encoded quantity.
///
/// `Lanczos3` matches the existing precedent in
/// `src/plugins/window_icon.rs`. At these ratios the practical difference
/// between Lanczos3, CatmullRom and Triangle is a few percent; the win is
/// resampling on the CPU at all rather than the specific kernel.
///
/// Returns the buffer untouched when it is already at or below the
/// target, so this is a no-op for sources that are already small.
fn downscale_icon_rgba(processed: &[u8], w: u32, h: u32, target: u32) -> (Vec<u8>, u32, u32) {
    if w == 0 || h == 0 || (w <= target && h <= target) {
        return (processed.to_vec(), w, h);
    }
    let alpha: Vec<u8> = processed.iter().skip(3).step_by(4).copied().collect();
    let Some(gray) = image::GrayImage::from_raw(w, h, alpha) else {
        // Buffer wasn't w*h*4 — leave it alone rather than corrupt it.
        return (processed.to_vec(), w, h);
    };
    // The icons are square today, but preserve aspect so a non-square
    // source doesn't silently get stretched.
    let (tw, th) = if w >= h {
        (target, (target as u64 * h as u64 / w as u64).max(1) as u32)
    } else {
        ((target as u64 * w as u64 / h as u64).max(1) as u32, target)
    };
    let small = image::imageops::resize(&gray, tw, th, image::imageops::FilterType::Lanczos3);
    let mut out = Vec::with_capacity(tw as usize * th as usize * 4);
    for px in small.pixels() {
        out.extend_from_slice(&[255, 255, 255, px.0[0]]);
    }
    (out, tw, th)
}

/// Baked texture edge for the bevy_ui path. The Build/Mining card cost
/// chips draw their icons at 20 logical px (`construction.rs`), so 64 px
/// covers them up to a 3× scale factor with headroom.
///
/// Fixed rather than DPI-derived like the egui side: this loader runs
/// once at startup off the `AssetServer` and has no egui context to ask
/// for `pixels_per_point`, and the residual 64→20 minification is mild
/// enough that the bilinear sampler handles it cleanly.
const BEVY_ICON_TEXTURE_SIZE: u32 = 64;

/// Rewrites a decoded bevy_ui icon `Image` to white-RGB + original alpha
/// and resamples it down to `BEVY_ICON_TEXTURE_SIZE`.
///
/// Same rationale as `downscale_icon_rgba` on the egui side: the source
/// PNGs are 1024×1024 and these draw at 20 px, a 51:1 minification that
/// the sampler cannot do cleanly without a mip chain. Returns `false` if
/// the buffer isn't tightly-packed RGBA8, in which case the caller should
/// mark the icon processed rather than retry forever.
fn process_and_downscale_bevy_icon(image: &mut Image) -> bool {
    let w = image.texture_descriptor.size.width;
    let h = image.texture_descriptor.size.height;
    let Some(data) = image.data.as_mut() else {
        return false;
    };
    if w == 0 || h == 0 || data.len() != (w as usize).saturating_mul(h as usize) * 4 {
        return false;
    }
    // RGB → pure white so a tinted `ImageNode` reads as the category
    // colour; alpha already carries the line (icons are pre-baked).
    for chunk in data.chunks_exact_mut(4) {
        chunk[0] = 255;
        chunk[1] = 255;
        chunk[2] = 255;
    }
    let (resized, tw, th) = downscale_icon_rgba(data, w, h, BEVY_ICON_TEXTURE_SIZE);
    if tw != w || th != h {
        *data = resized;
        image.texture_descriptor.size = Extent3d {
            width: tw,
            height: th,
            depth_or_array_layers: 1,
        };
    }
    true
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

/// Convenience: returns the dedicated energy icon's egui
/// `TextureHandle`, or `None` if the PNG hasn't been decoded yet
/// (the render side falls back to a tinted square in that case).
/// Energy is not a `ResourceType` so it lives outside
/// `ResourceIcons::handles`.
pub fn get_energy_icon_handle<'a>(icons: &'a ResourceIcons) -> Option<&'a egui::TextureHandle> {
    icons.energy_handle.as_ref()
}

/// Convenience: returns the bevy_ui `Handle<Image>` for the
/// energy icon, or `None` if the asset hasn't loaded yet. The
/// Build/Mining card render side falls back to a tinted square
/// in that case (same fallback as `get_resource_icon_handle_bevy`).
///
/// Forward-declared for the upcoming per-row energy demand /
/// production display on the canary Build / Mining cards. Not
/// called yet — silence the dead-code lint until the card side
/// wires it in.
#[allow(dead_code)]
pub fn get_energy_icon_handle_bevy(icons: &ResourceIcons) -> Option<&Handle<Image>> {
    icons.energy_bevy_handle.as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a `w`×`h` RGBA buffer whose alpha is a 1-px-wide vertical
    /// stripe every `period` columns — the pathological case for
    /// minification, and a fair stand-in for the thin line art in the
    /// category badges.
    fn striped_rgba(w: u32, h: u32, period: u32) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..h {
            for x in 0..w {
                let a = if x % period == 0 { 255 } else { 0 };
                v.extend_from_slice(&[255, 255, 255, a]);
            }
        }
        v
    }

    #[test]
    fn downscale_reduces_to_target_and_keeps_rgba_layout() {
        let src = striped_rgba(1024, 1024, 16);
        let (out, w, h) = downscale_icon_rgba(&src, 1024, 1024, 64);
        assert_eq!((w, h), (64, 64));
        assert_eq!(out.len(), 64 * 64 * 4);
        // RGB must stay pure white so the egui/bevy tint shader still
        // produces the category colour.
        assert!(out.chunks_exact(4).all(|c| c[0] == 255 && c[1] == 255 && c[2] == 255));
    }

    #[test]
    fn downscale_preserves_mean_coverage() {
        // The whole point: an area-integrating filter conserves total ink.
        // A 2x2 bilinear tap on this input would sample only whole
        // stripes or whole gaps and land nowhere near the true mean.
        let src = striped_rgba(1024, 1024, 16);
        let src_mean =
            src.iter().skip(3).step_by(4).map(|&a| a as f64).sum::<f64>() / (1024.0 * 1024.0);
        let (out, w, h) = downscale_icon_rgba(&src, 1024, 1024, 64);
        let out_mean = out.iter().skip(3).step_by(4).map(|&a| a as f64).sum::<f64>()
            / (w as f64 * h as f64);
        assert!(
            (src_mean - out_mean).abs() < 2.0,
            "coverage drifted: {src_mean:.2} -> {out_mean:.2}"
        );
    }

    #[test]
    fn downscale_is_a_noop_when_already_small() {
        let src = striped_rgba(32, 32, 4);
        let (out, w, h) = downscale_icon_rgba(&src, 32, 32, 64);
        assert_eq!((w, h), (32, 32));
        assert_eq!(out, src);
    }

    #[test]
    fn downscale_preserves_aspect_ratio() {
        let src = striped_rgba(512, 256, 8);
        let (_, w, h) = downscale_icon_rgba(&src, 512, 256, 64);
        assert_eq!((w, h), (64, 32));
    }

    #[test]
    fn downscale_rejects_malformed_buffer_without_corrupting_it() {
        // Not w*h*4 bytes — must hand the buffer back untouched rather
        // than build a `GrayImage` from a short slice.
        let src = vec![255u8; 10];
        let (out, w, h) = downscale_icon_rgba(&src, 1024, 1024, 64);
        assert_eq!((w, h), (1024, 1024));
        assert_eq!(out, src);
    }

    #[test]
    fn icon_texture_size_scales_with_dpi_and_clamps() {
        assert_eq!(icon_texture_size(1.0), 32);
        assert_eq!(icon_texture_size(2.0), 64);
        assert_eq!(icon_texture_size(1.5), 48);
        // Degenerate scale factors during window init must not panic or
        // produce a zero-sized texture.
        assert_eq!(icon_texture_size(0.0), ICON_TEXTURE_MIN);
        assert_eq!(icon_texture_size(f32::NAN), ICON_TEXTURE_MIN);
        assert_eq!(icon_texture_size(100.0), ICON_TEXTURE_MAX);
    }
}
