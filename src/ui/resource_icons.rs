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
use std::future::Future;

use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::Extent3d;
use bevy_egui::egui;

use crate::economy::ResourceType;
use crate::ui::icon_cache::{
    self, cache_dir, fnv1a_hex, load_manifest, save_manifest, validate, CacheValidation,
    IconCacheManifest,
};

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
    /// The disk cache manifest for the icon cache. `None` until
    /// the first `Update` after the bevy_ui loader inserted the
    /// resource. Populated lazily by [`load_resource_icons`].
    pub cache_manifest: Option<IconCacheManifest>,
    /// True once the cache has been validated (and, on a cold
    /// cache, the async bake has completed). Set by
    /// [`bootstrap_icon_cache`].
    pub cache_ready: bool,
    /// In-flight background bake task (cold cache / stale sources).
    /// The bake decodes each 1024×1024 source + Lanczos-downscales
    /// it to 7 sizes ≈ ~1.8 s per icon in the `fast` profile — FAR
    /// too heavy for the main thread. It runs on
    /// [`AsyncComputeTaskPool`] so frame 1 of a cold launch pays a
    /// few ms of validation, not a ~20 s stall. `None` once the
    /// task completes or the cache was fresh.
    pub bake_task: Option<bevy::tasks::Task<Result<(), String>>>,
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
    // v0.5.2 PR-A.7 (2026-08-06): the on-disk category icons are
    // now pre-baked 64×64 white-on-transparent PNGs (cropped to
    // their content bounding box, no padding). The runtime is a
    // straight load + NEAREST-display; no luminance key, no
    // downscale dance. Earlier 1024→64 / 1024→32 bake pipelines
    // were a workaround for a thin-strokes problem that turned
    // out to be an authoring issue, not a display-size issue.
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
    let energy_bevy_handle: Handle<Image> = asset_server.load("textures/ui/resources/energy.png");
    commands.insert_resource(ResourceIcons {
        handles: HashMap::new(), // egui path remains untouched
        bevy_handles: handles,
        bevy_pending: pending,
        category_handles: HashMap::new(), // populated by load_resource_icons (egui path)
        energy_handle: None,              // populated by load_resource_icons (egui path)
        energy_bevy_handle: Some(energy_bevy_handle),
        energy_bevy_pending: true,
        texture_size: 0,      // forces the egui loader to bake on its first frame
        cache_manifest: None, // validated lazily on the first egui frame
        cache_ready: false,
        bake_task: None,
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
    // The bevy_ui post-processor may run before the egui loader has
    // bootstrapped the cache (both are in `Update`, unordered). Ensure
    // the manifest is loaded + baked so the cache fast-path below can
    // hit on the first frame.
    bootstrap_icon_cache(&mut icons);
    // Snapshot the (resource, handle) pairs we still need to
    // process — avoids borrow conflicts on the `icons` map while
    // we mutate `Assets<Image>`.
    let candidates: Vec<(ResourceType, Handle<Image>)> = icons
        .bevy_handles
        .iter()
        .filter(|(r, _)| icons.bevy_pending.contains(r))
        .map(|(r, h)| (*r, h.clone()))
        .collect();

    // Process AT MOST ONE pre-baked resource icon per frame.
    //
    // ## Why (GRA regression, 2026-08-05)
    //
    // Each icon is a 1024×1024 RGBA image (4.2M pixels). The old
    // code processed every pending icon in a single `Update` tick,
    // and the icons load asynchronously — the frame where the whole
    // batch finally became available ran ~38 × (4M-pixel RGB→white
    // loop + alpha extraction + Lanczos3 1024→64 downscale) inline,
    // which blocked the main thread for ~20 s (the "splash black
    // box" at startup: frame 0's Update→PostUpdate boundary stalls).
    //
    // Processing one icon per frame spreads the same total work over
    // ~38 frames (~0.6 s of real time at 60 fps) with no single-frame
    // stall. Icons that aren't decoded yet are naturally retried on
    // later frames, and the per-icon cost is tiny once the first one
    // has been processed (the loop below early-returns on frames with
    // nothing pending).
    //
    // ## Cache fast-path (2026-08-05)
    //
    // When the disk cache is ready (see [`load_resource_icons`]), the
    // 64 px baked PNG is read directly and copied into the `Image`
    // buffer — no 1024×1024 downscale, no luminance key. The cached
    // bytes are already white-RGB + alpha-keyed. Falls back to the
    // inline `process_and_downscale_bevy_icon` when the cache is
    // missing (first-ever launch, or a cache-write race).
    if let Some((resource, handle)) = candidates.into_iter().next() {
        let Some(image) = images.get_mut(&handle) else {
            // AssetServer hasn't decoded the PNG yet; try again
            // next frame. Skipping is the right behaviour — we
            // can't mutate a buffer we don't have, and forcing
            // the load would block the schedule.
            return;
        };
        // Icons are pre-baked (see scripts/bake_resource_icons.py):
        //   - RGB is pure black (0,0,0) for the line
        //   - Alpha is the line opacity: 0 on the white
        //     background, 255 on the solid line, linear
        //     ramp on antialiased edges.
        // The runtime resamples 1024 px → 64 px so the 20 px card
        // chip isn't minified 51:1 by a mip-less bilinear sampler.
        //
        // A `false` return means the wrong pixel format (compressed,
        // sRGB-float, …) — mark processed to avoid retrying every frame.
        let cache_hit = if let Some(manifest) = icons.cache_manifest.clone() {
            let key = resource_cache_key(resource);
            cached_icon_path(&Some(manifest), &key, BEVY_ICON_TEXTURE_SIZE)
                .and_then(|p| load_cached_png_rgba(&p))
                .map(|(bytes, w, h)| {
                    apply_cached_bevy_icon(image, &bytes, w, h);
                    true
                })
                .unwrap_or(false)
        } else {
            false
        };
        if !cache_hit {
            process_and_downscale_bevy_icon(image);
        }
        icons.bevy_pending.remove(&resource);
        // Only one icon per frame — return now; the energy icon gets
        // its own frame (see below).
        return;
    }

    // Dedicated energy icon. v0.5.2 PR-A.7 (2026-08-06): the
    // on-disk `energy.png` is already a 64×64 white-on-transparent
    // PNG (cropped to its content bounding box, luminance key applied
    // at author time). The bevy_ui path just needs to copy the cache
    // bytes in (if available) or apply the same RGB→white + alpha
    // passthrough the egui path uses. No per-pixel luminance key
    // needed at runtime.
    if icons.energy_bevy_pending {
        let Some(handle) = icons.energy_bevy_handle.clone() else {
            icons.energy_bevy_pending = false;
            return;
        };
        let Some(image) = images.get_mut(&handle) else {
            // AssetServer hasn't decoded yet; try again next frame.
            return;
        };
        // Cache fast-path: the 64 px baked energy icon is already
        // white-on-transparent; copy it straight in.
        let cache_hit = if let Some(manifest) = icons.cache_manifest.clone() {
            cached_icon_path(
                &Some(manifest),
                icon_cache::ENERGY_KEY,
                BEVY_ICON_TEXTURE_SIZE,
            )
            .and_then(|p| load_cached_png_rgba(&p))
            .map(|(bytes, w, h)| {
                apply_cached_bevy_icon(image, &bytes, w, h);
                true
            })
            .unwrap_or(false)
        } else {
            false
        };
        if cache_hit {
            icons.energy_bevy_pending = false;
            return;
        }
        // Inline path: rewrite RGB to white (the AssetServer-decoded
        // 64 px file has whatever RGB the synthesis tool emitted —
        // for the new pre-baked format that's already white, but the
        // rewrite is idempotent and protects against a rolled-back
        // source format). Alpha is passed through as-is.
        if let Some(data) = image.data.as_mut() {
            let w = image.texture_descriptor.size.width;
            let h = image.texture_descriptor.size.height;
            if w > 0 && h > 0 && data.len() == (w as usize).saturating_mul(h as usize) * 4 {
                for chunk in data.chunks_exact_mut(4) {
                    chunk[0] = 255;
                    chunk[1] = 255;
                    chunk[2] = 255;
                    // Alpha unchanged.
                }
            }
        }
        // The image is already 64 px (matches BEVY_ICON_TEXTURE_SIZE);
        // process_and_downscale_bevy_icon's downscale is a 1:1
        // no-op for a 64 px source. Still call it for the
        // RGB→white pass on the alpha-channel extraction path
        // (the function re-emits white RGB + alpha).
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
///
/// ## Cache integration (2026-08-05)
///
/// On the first frame that runs, the loader validates the disk icon
/// cache (`<userdata>/cache/resource_icons/`) against the current
/// source PNGs. If fresh, every icon is read from a tiny cached PNG
/// (64 px) instead of re-decoding + re-processing 1024×1024 sources.
/// If stale, the stale keys are re-baked once (all DPI sizes) and
/// the manifest is rewritten atomically; the per-frame budget
/// (`MAX_ICONS_PER_FRAME`) still applies to the bake.
pub fn load_resource_icons(
    mut contexts: bevy_egui::EguiContexts,
    mut icons: ResMut<ResourceIcons>,
    mut needs: ResMut<ResourceIconNeeds>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let ctx = ctx.clone();

    // ── 0. Cache bootstrap (once per process) ───────────────────
    bootstrap_icon_cache(&mut icons);

    // Lazy on-demand (v0.5.2, 2026-08-05): this system is gated on
    // `in_game_chrome` (see `src/ui/mod.rs`), so running it means
    // the player is in-game and the resources bar will draw the full
    // set. Declare everything as needed once; the per-key bake loops
    // below then do the actual work. (A finer-grained mark-on-hover
    // could trim this further, but the resources bar always shows
    // every resource when in-game, so the full-set mark is correct.)
    if !needs.wants_all() {
        for &resource in ResourceType::all() {
            needs.mark_resource(resource);
        }
        for (category, _) in ResourceType::by_category() {
            needs.mark_category(category);
        }
        needs.mark_energy();
    }

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

    // Process a bounded number of icons per frame so the initial
    // load doesn't stall the main thread. Each icon is a 1024×1024
    // RGBA image; loading + luminance/white processing + Lanczos3
    // downscale for all ~47 icons in one tick blocked frame 0 for
    // ~20 s (the GRA splash-stall regression fixed 2026-08-05).
    // Spreading the same work over ~24 frames removes the single
    // long frame entirely; the loop below resumes where it left off
    // because processed icons are skipped via `handles.contains_key`.
    //
    // ## Warm-cache fast path (2026-08-05, bugfix round 2)
    //
    // The budget exists for the COLD path (1024×1024 source decode +
    // luminance key + Lanczos). When the disk cache is READY, each
    // icon is a tiny 64 px PNG read + decode — microseconds. Loading
    // 2/frame would trickle the resource-bar icons in over ~25 frames
    // AFTER the boot overlay hid, producing the "icons pop up one by
    // one already in game" the player reported. So when
    // `cache_ready`, the budget is effectively unlimited: all icons
    // load in a single frame, and the boot overlay (which waits for
    // `all_needed_loaded`) stays up until they're all in.
    const MAX_ICONS_PER_FRAME: usize = 2;
    // `usize::MAX` when warm → no per-frame cap.
    let per_frame_budget = if icons.cache_ready {
        usize::MAX
    } else {
        MAX_ICONS_PER_FRAME
    };
    let mut processed_this_frame = 0usize;

    let manifest = icons.cache_manifest.clone();

    // ── 2. Resource icons from cache ─────────────────────────────
    for &resource in ResourceType::all() {
        if processed_this_frame >= per_frame_budget {
            break;
        }
        if icons.handles.contains_key(&resource) {
            continue;
        }
        // Lazy on-demand: only bake what the UI has actually asked
        // to draw this session (see [`ResourceIconNeeds`]).
        if !needs.resources.contains(&resource) {
            continue;
        }
        let key = resource_cache_key(resource);
        // Try the cache first.
        if let Some(path) = cached_icon_path(&manifest, &key, target) {
            match load_cached_icon(&ctx, &path, &format!("resource_icon_{:?}", resource)) {
                Some(handle) => {
                    icons.handles.insert(resource, handle);
                    processed_this_frame += 1;
                }
                None => continue,
            }
            continue;
        }
        // Cache miss (missing source or manifest gap) — fall back
        // to the inline decode so the icon still appears.
        let Some(path) = resource_icon_path(resource) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&path) else {
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
            egui::TextureOptions::NEAREST,
        );
        icons.handles.insert(resource, handle);
        processed_this_frame += 1;
    }

    // ── 3. Category badges from cache ────────────────────────────
    // Same pattern for the 9 category-badge PNGs
    // (`category-atmospheric.png`, `category-biological.png`, …).
    // The `by_category()` list is the source of truth for which
    // categories exist; `category_icon_basename()` maps each name
    // to its on-disk slug. Categories without a slug (a new
    // category added before the icon is authored) silently fall
    // through to the placeholder.
    for (category, _) in ResourceType::by_category() {
        if processed_this_frame >= per_frame_budget {
            break;
        }
        if icons.category_handles.contains_key(category) {
            continue;
        }
        // Lazy on-demand: only bake categories the UI draws.
        if !needs.categories.contains(category) {
            continue;
        }
        let Some(slug) = category_icon_basename(category) else {
            continue;
        };
        let key = format!("category:{}", category);
        if let Some(path) = cached_icon_path(&manifest, &key, target) {
            match load_cached_icon(&ctx, &path, &format!("category_icon_{}", category)) {
                Some(handle) => {
                    icons.category_handles.insert(category.to_string(), handle);
                    processed_this_frame += 1;
                }
                None => continue,
            }
            continue;
        }
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
            egui::TextureOptions::NEAREST,
        );
        icons.category_handles.insert(category.to_string(), handle);
        processed_this_frame += 1;
    }

    // ── 4. Energy icon from cache ────────────────────────────────
    // Dedicated energy icon (`assets/textures/ui/resources/energy.png`).
    // Energy is not a `ResourceType` — it's the power-balance concept
    // used by the top resource bar's "TW" chip and (eventually) the
    // Build/Mining card energy rows. Loaded with the same
    // luminance-key recipe as the category badges (dark navy on white
    // → premultiplied white on transparent) and tinted at the call
    // site: green/red for the power chip, cyan by default elsewhere.
    if icons.energy_handle.is_none() && needs.energy {
        if let Some(path) = cached_icon_path(&manifest, icon_cache::ENERGY_KEY, target) {
            if let Some(handle) = load_cached_icon(&ctx, &path, "energy_icon") {
                icons.energy_handle = Some(handle);
            }
        } else {
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
                        egui::TextureOptions::NEAREST,
                    );
                    icons.energy_handle = Some(handle);
                }
            }
        }
    }
}

/// Resolve the disk-cache path for `key` at `size`, from the loaded
/// manifest. Returns `None` when the cache has no baked output (the
/// caller falls back to inline processing).
fn cached_icon_path(
    manifest: &Option<IconCacheManifest>,
    key: &str,
    size: u32,
) -> Option<std::path::PathBuf> {
    icon_cache::cached_output(manifest, &cache_dir(), key, size)
}

/// Load the cache manifest + validate + (if stale) spawn a
/// background bake. Runs every frame until the cache is ready —
/// both [`load_resource_icons`] (egui) and
/// [`post_process_resource_icons`] (bevy_ui) call it, whichever
/// runs first does the work and flips `cache_ready`.
///
/// ## Async bake (2026-08-05)
///
/// A cold cache (first launch after this feature) has every key
/// stale. Each bake decodes a 1024×1024 source + Lanczos-downscales
/// it to 7 sizes — ~1.8 s per icon in the `fast` profile — so the
/// ~49-icon batch would block the main thread for ~90 s if baked
/// inline, and even a 2-per-frame budget produced ~3.6 s hitches.
/// The bake therefore runs on [`AsyncComputeTaskPool`]: frame 1
/// pays only the ~1 ms validation, the bake progresses on a
/// background thread, and `cache_ready` flips when it lands. Icons
/// display immediately via the inline (budgeted) fallback while the
/// background bake warms the cache for subsequent launches.
fn bootstrap_icon_cache(icons: &mut ResourceIcons) {
    let sources = all_icon_sources();
    bootstrap_icon_cache_with_sources(icons, &sources);
}

/// Shared body of [`bootstrap_icon_cache`] with an injectable source
/// map (tests pass synthetic tiny sources so the async bake finishes
/// in milliseconds instead of ~90 s).
fn bootstrap_icon_cache_with_sources(
    icons: &mut ResourceIcons,
    sources: &HashMap<String, std::path::PathBuf>,
) {
    if icons.cache_ready {
        return;
    }

    // First call: load the manifest + validate. Cheap (~49 stat
    // calls + content hashes only on stat-match surprises).
    if icons.cache_manifest.is_none() {
        icons.cache_manifest = load_manifest(&cache_dir()).ok().flatten();
    }
    if icons.bake_task.is_none() {
        let validation = validate(&cache_dir(), &icons.cache_manifest, sources, |p| {
            std::fs::read(p).ok().map(|b| fnv1a_hex(&b))
        });
        let needs_bake = match validation {
            CacheValidation::Fresh => Vec::new(),
            CacheValidation::Stale { needs_bake } => needs_bake,
        };
        if needs_bake.is_empty() {
            // Nothing to bake — the cache is ready immediately.
            icons.cache_ready = true;
            return;
        }
        info!(
            "icon cache: {} stale icon set(s) — baking on AsyncComputeTaskPool",
            needs_bake.len()
        );
        let pool = bevy::tasks::AsyncComputeTaskPool::get();
        let cache_dir = cache_dir();
        let sources = sources.clone();
        let task = pool.spawn(async move {
            bake_keys(&cache_dir, &sources, &needs_bake);
            Ok(())
        });
        icons.bake_task = Some(task);
        return; // task in flight; poll on a later frame
    }

    // Poll the in-flight bake (non-blocking, same pattern as the
    // `solar_system.ron` pre-parse in `boot_init.rs`).
    let Some(task) = icons.bake_task.as_mut() else {
        return;
    };
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    match std::pin::Pin::new(task).poll(&mut cx) {
        std::task::Poll::Ready(Ok(())) => {
            // Reload the persisted manifest so `cached_icon_path`
            // resolves for the DPI-rebake path.
            icons.cache_manifest = load_manifest(&cache_dir()).ok().flatten();
            icons.bake_task = None;
            icons.cache_ready = true;
            info!("icon cache: async bake complete — cache ready");
        }
        std::task::Poll::Ready(Err(err)) => {
            warn!("icon cache: async bake failed ({err}); using inline path only");
            icons.bake_task = None;
            icons.cache_ready = true;
        }
        std::task::Poll::Pending => {
            // Still baking; try again next frame.
        }
    }
}

/// Bake a batch of logical keys to every cache size, persisting the
/// manifest after EACH key so an interrupted bake (player quits
/// before the full ~90 s background pass finishes) keeps its
/// progress — the next launch validates the completed entries as
/// fresh and only re-bakes the remainder. Runs entirely on the async
/// pool (pure CPU + `std::fs`; no ECS access). Partial failures skip
/// the broken key and keep going — a corrupt source just won't be
/// cached.
///
/// The cache dir is created FIRST — `bake_one_key` writes PNGs via
/// `File::create` and would silently fail every write (and produce
/// an empty manifest) if the directory didn't exist yet.
fn bake_keys(
    cache_dir: &std::path::Path,
    sources: &HashMap<String, std::path::PathBuf>,
    keys: &[String],
) {
    if let Err(e) = std::fs::create_dir_all(cache_dir) {
        warn!(
            "icon cache: could not create {} ({e}); bake aborted",
            cache_dir.display()
        );
        return;
    }
    // Start from the manifest already on disk so a resume keeps the
    // entries completed by a previous (interrupted) bake.
    let mut manifest = load_manifest(cache_dir).ok().flatten().unwrap_or_default();
    // Stamp the schema version — `Default` yields 0, but the
    // validator rejects anything != CACHE_VERSION as stale, so a
    // manifest written with version 0 would be re-baked on every
    // launch (the "49 stale every time" bug found in smoke test 6).
    manifest.version = icon_cache::CACHE_VERSION;
    for key in keys {
        bake_one_key(cache_dir, sources, key, &mut manifest);
        // Persist after each key — an interrupted bake never loses
        // the keys completed so far.
        let _ = save_manifest(cache_dir, &manifest);
    }
}

/// Load + register a cached PNG as an egui texture. Returns `None`
/// on any read/decode failure so the caller falls back to inline.
fn load_cached_icon(
    ctx: &egui::Context,
    path: &std::path::Path,
    texture_name: &str,
) -> Option<egui::TextureHandle> {
    let bytes = std::fs::read(path).ok()?;
    let image = image::load_from_memory(&bytes).ok()?;
    let rgba = image.to_rgba8();
    let (w, h) = rgba.dimensions();
    let color_image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
    Some(ctx.load_texture(
        texture_name.to_string(),
        color_image,
        egui::TextureOptions::NEAREST,
    ))
}

/// Load a cached PNG and return its decoded RGBA bytes + size.
/// Used by the bevy_ui path to copy the 64 px baked icon into an
/// `Assets<Image>` buffer without re-processing 1024×1024 sources.
fn load_cached_png_rgba(path: &std::path::Path) -> Option<(Vec<u8>, u32, u32)> {
    let bytes = std::fs::read(path).ok()?;
    let image = image::load_from_memory(&bytes).ok()?;
    let rgba = image.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    Some((rgba.into_raw(), w, h))
}

/// Copy cached RGBA bytes into a bevy_ui `Image`, replacing its
/// texture descriptor + data with the baked size. The cached bytes
/// are already white-RGB + alpha-keyed, so this is a straight copy —
/// no luminance key, no downscale.
fn apply_cached_bevy_icon(image: &mut Image, rgba: &[u8], w: u32, h: u32) {
    image.data = Some(rgba.to_vec());
    image.texture_descriptor.size = Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };
}

/// Logical cache key for a resource icon.
pub(crate) fn resource_cache_key(resource: ResourceType) -> String {
    format!("resource:{}", resource.display_name())
}

/// Tracks which egui resource icons the UI has actually asked to
/// draw this session.
///
/// ## Why (2026-08-05)
///
/// The egui resource-icon loop ran every `Update` frame, decoding all
/// 39 resource PNGs + 9 category badges + energy from disk even while
/// the player sat on the main menu (where none of those icons render).
/// With the disk cache this is now cheap, but it is still pure waste
/// in the menu. [`ResourceIconNeeds`] lets the render sites declare
/// what they actually need:
///
/// - The resources bar marks every resource + category + energy on
///   draw (it shows the whole set when in-game).
/// - A construction card hover marks just that card's resource
///   demands.
///
/// [`load_resource_icons`] bakes only the marked keys; anything not
/// marked is skipped until a render site asks for it. Combined with
/// the `in_game_chrome` gate (menu → the loop doesn't run at all),
/// the menu does zero icon work.
#[derive(Resource, Default)]
pub struct ResourceIconNeeds {
    pub resources: std::collections::HashSet<ResourceType>,
    pub categories: std::collections::HashSet<String>,
    pub energy: bool,
}

impl ResourceIconNeeds {
    /// Declare that a resource icon will be drawn this session.
    pub fn mark_resource(&mut self, resource: ResourceType) {
        self.resources.insert(resource);
    }

    /// Declare that a category-badge icon will be drawn.
    pub fn mark_category(&mut self, category: &str) {
        self.categories.insert(category.to_string());
    }

    /// Declare that the energy icon will be drawn.
    pub fn mark_energy(&mut self) {
        self.energy = true;
    }

    /// True when every icon in `ResourceType::all()` has been marked.
    pub fn wants_all(&self) -> bool {
        ResourceType::all()
            .iter()
            .all(|r| self.resources.contains(r))
    }
}

impl ResourceIcons {
    /// True when every icon the UI has declared needed is present in
    /// the loaded maps (resources + categories + energy).
    ///
    /// Used by the boot overlay (v0.5.2, 2026-08-05): the overlay
    /// hides only when BOTH the boot chain is `Ready` AND every icon
    /// the resources bar draws has landed, so the player never sees
    /// the resource-bar icons pop in one by one after the "Generating
    /// world" bar finishes.
    pub fn all_needed_loaded(&self, needs: &ResourceIconNeeds) -> bool {
        needs.resources.iter().all(|r| self.handles.contains_key(r))
            && needs
                .categories
                .iter()
                .all(|c| self.category_handles.contains_key(c))
            && (!needs.energy || self.energy_handle.is_some())
    }
}

/// Build the `logical key → source path` map used for cache
/// validation + baking. Includes every resource, category badge and
/// the energy icon.
fn all_icon_sources() -> HashMap<String, std::path::PathBuf> {
    let mut sources = HashMap::new();
    for &resource in ResourceType::all() {
        let Some(path) = resource_icon_path(resource) else {
            continue;
        };
        sources.insert(resource_cache_key(resource), std::path::PathBuf::from(path));
    }
    for (category, _) in ResourceType::by_category() {
        let Some(slug) = category_icon_basename(category) else {
            continue;
        };
        sources.insert(
            format!("category:{}", category),
            std::path::PathBuf::from("assets/textures/ui/resources").join(format!("{}.png", slug)),
        );
    }
    sources.insert(
        icon_cache::ENERGY_KEY.to_string(),
        std::path::PathBuf::from("assets/textures/ui/resources/energy.png"),
    );
    sources
}

/// Bake ONE logical key to every cache size and insert the entry
/// into `manifest`. Does NOT persist — the caller
/// ([`bake_keys`]) saves once after the whole batch.
///
/// Missing sources are recorded as `missing: true` (so the runtime
/// skips them instead of failing every frame).
///
/// This is the *only* place the full 1024×1024 source decode happens.
/// Every other launch path reads the baked 64 px PNGs. Each call
/// decodes one source + Lanczos-downscales it to 7 sizes (~1.8 s in
/// the `fast` profile) — hence the async pool in
/// [`bootstrap_icon_cache`].
fn bake_one_key(
    cache_dir: &std::path::Path,
    sources: &HashMap<String, std::path::PathBuf>,
    key: &str,
    manifest: &mut IconCacheManifest,
) {
    let Some(source_path) = sources.get(key) else {
        return;
    };
    let Some(stat) = crate::ui::icon_cache::SourceStat::of(source_path) else {
        // Missing source — record the absence so we stop asking
        // every frame.
        manifest.entries.insert(
            key.to_string(),
            crate::ui::icon_cache::IconCacheEntry {
                source_path: source_path.display().to_string(),
                source_stat: crate::ui::icon_cache::SourceStat {
                    len: 0,
                    mtime_ns: 0,
                },
                source_hash: String::new(),
                outputs: HashMap::new(),
                missing: true,
            },
        );
        return;
    };
    let Ok(bytes) = std::fs::read(source_path) else {
        return;
    };
    let Ok(image) = image::load_from_memory(&bytes) else {
        return;
    };
    let rgba = image.to_rgba8();
    let (w, h) = rgba.dimensions();
    // v0.5.2 PR-A.7 (2026-08-06): the on-disk source is already
    // a 64×64 white-on-transparent PNG (cropped to its content
    // bounding box, luminance key applied at author time). No
    // pre-bake, no luminance-key recomputation — both post-process
    // functions are now RGB→white + alpha passthrough (idempotent
    // on a pre-baked source).
    let is_resource = key.starts_with("resource:");
    let processed = if is_resource {
        post_process_rgba(rgba.as_raw())
    } else {
        post_process_category_rgba(rgba.as_raw())
    };

    let mut outputs = HashMap::new();
    let safe_key = icon_cache::sanitize_filename(key);
    for &size in icon_cache::ICON_CACHE_SIZES {
        let (small, tw, th) = downscale_icon_rgba(&processed, w, h, size);
        let (tw, th) = (tw.max(1), th.max(1));
        let Some(img) = image::RgbaImage::from_raw(tw, th, small) else {
            return;
        };
        // The file name must be a sanitized fragment — the logical
        // key contains `:` (illegal on Windows / ADS separator), so
        // `resource:Water_32.png` would silently write an NTFS
        // stream named `Water_32.png` on the file `resource`. See
        // `icon_cache::sanitize_filename`.
        let file_name = format!("{safe_key}_{size}.png");
        let out_path = cache_dir.join(&file_name);
        let Ok(file) = std::fs::File::create(&out_path) else {
            return;
        };
        if image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::BufWriter::new(file), image::ImageFormat::Png)
            .is_err()
        {
            return;
        }
        outputs.insert(size, file_name);
    }

    manifest.entries.insert(
        key.to_string(),
        crate::ui::icon_cache::IconCacheEntry {
            source_path: source_path.display().to_string(),
            source_stat: stat,
            source_hash: fnv1a_hex(&bytes),
            outputs,
            missing: false,
        },
    );
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
/// RGB white for egui tinting.
///
/// ## v0.5.2 PR-A.7 final (2026-08-06): pass-through
///
/// The on-disk category icons are pre-baked 64×64 white-on-transparent
/// PNGs (cropped to their content bounding box, luminance key applied
/// at author time by `scripts/one_shot_bake2.py`). The runtime no
/// longer needs to do any per-pixel work — this function is a
/// RGB→white + alpha passthrough, identical to `post_process_rgba`.
///
/// The two functions exist as separate symbols so a future change
/// (e.g. a different category source format, a per-category tint
/// override) has a clear place to add a recipe without touching the
/// resource path.
fn post_process_category_rgba(rgba: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; rgba.len()];
    for (src_chunk, dst_chunk) in rgba.chunks_exact(4).zip(out.chunks_exact_mut(4)) {
        dst_chunk[0] = 255;
        dst_chunk[1] = 255;
        dst_chunk[2] = 255;
        dst_chunk[3] = src_chunk[3];
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
    // v0.5.2 PR-A.7 (2026-08-06): the source icons are now
    // authored at 64 px (the `category-*-64.png` set), so the
    // runtime downscale is only 2:1 (64→32 at 1× DPI) or
    // 1:1 (64→64 at 2× DPI, which the early-return above
    // already skips). Lanczos3 is designed for 8:1+ ratios
    // (the original 1024→64 case) where it provides real
    // anti-aliased averaging. At 2:1 the Lanczos kernel is
    // wider than the downscale itself and just smears the
    // alpha across two destination texels, blurring thin
    // strokes into a semi-transparent haze.
    //
    // Switch to NEAREST for the 2:1 (and similar small) case
    // so each source texel maps cleanly to one destination
    // texel with no kernel bleed. For the legacy 1024 px
    // source (if anyone runs with the old icons) the ratio
    // is 16:1 and Lanczos3 still helps — the threshold at
    // 4:1 keeps both paths well-tuned.
    let downscale_ratio = (w as f64 / tw as f64).max(h as f64 / th as f64);
    let filter = if downscale_ratio > 4.0 {
        image::imageops::FilterType::Lanczos3
    } else {
        // Nearest preserves thin strokes at small ratios; the
        // 1-texel stair-stepping is invisible at 28-32 px
        // display sizes (the eye reads the shape, not the
        // individual texels).
        image::imageops::FilterType::Nearest
    };
    let small = image::imageops::resize(&gray, tw, th, filter);
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
/// v0.5.2 PR-A.7 (2026-08-06): the on-disk source is now a 64×64
/// white-on-transparent PNG (matches `BEVY_ICON_TEXTURE_SIZE`), so
/// the downscale step is a 1:1 no-op for the typical case. The
/// sampler is explicitly set to NEAREST so the 64→20 minification
/// at the draw call keeps thin strokes crisp (Bevy's default is
/// linear, which softens them).
///
/// Returns `false` if the buffer isn't tightly-packed RGBA8, in
/// which case the caller should mark the icon processed rather
/// than retry forever.
fn process_and_downscale_bevy_icon(image: &mut Image) -> bool {
    let w = image.texture_descriptor.size.width;
    let h = image.texture_descriptor.size.height;
    let Some(data) = image.data.as_mut() else {
        return false;
    };
    if w == 0 || h == 0 || data.len() != (w as usize).saturating_mul(h as usize) * 4 {
        return false;
    }
    // NOTE: we deliberately do NOT do a full-resolution RGB→white
    // pass here. `downscale_icon_rgba` extracts only the alpha
    // channel and re-emits pure-white RGB, so the per-pixel RGB
    // rewrite below would be pure waste on a 1024×1024 buffer
    // (4.2M pixels per icon × ~38 icons ≈ a multi-second stall
    // when the icons first become available — the GRA regression
    // fixed 2026-08-05). The energy icon's luminance key has
    // already rewritten alpha before this call, and the pre-baked
    // icons carry their line in alpha. Skipping the RGB pass is
    // free and correct.
    let (resized, tw, th) = downscale_icon_rgba(data, w, h, BEVY_ICON_TEXTURE_SIZE);
    if tw != w || th != h {
        *data = resized;
        image.texture_descriptor.size = Extent3d {
            width: tw,
            height: th,
            depth_or_array_layers: 1,
        };
    }
    // Force NEAREST sampling so the 64→20-ish minification at the
    // draw call keeps thin strokes crisp. Bevy's default is linear,
    // which softens 1-2 px line art into a gray haze.
    image.sampler = ImageSampler::nearest();
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
pub fn get_energy_icon_handle(icons: &ResourceIcons) -> Option<&egui::TextureHandle> {
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
        assert!(out
            .chunks_exact(4)
            .all(|c| c[0] == 255 && c[1] == 255 && c[2] == 255));
    }

    #[test]
    fn downscale_preserves_mean_coverage() {
        // The whole point: an area-integrating filter conserves total ink.
        // A 2x2 bilinear tap on this input would sample only whole
        // stripes or whole gaps and land nowhere near the true mean.
        let src = striped_rgba(1024, 1024, 16);
        let src_mean = src
            .iter()
            .skip(3)
            .step_by(4)
            .map(|&a| a as f64)
            .sum::<f64>()
            / (1024.0 * 1024.0);
        let (out, w, h) = downscale_icon_rgba(&src, 1024, 1024, 64);
        let out_mean = out
            .iter()
            .skip(3)
            .step_by(4)
            .map(|&a| a as f64)
            .sum::<f64>()
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

    /// Regression guard for the 2026-08-05 splash-stall: a cold cache
    /// must NOT bake icons on the main thread. The bake (decode +
    /// 7× Lanczos downscale per icon ≈ ~1.8 s/icon in the `fast`
    /// profile) runs on [`AsyncComputeTaskPool`], so the first
    /// `bootstrap_icon_cache` call only validates (~1 ms) and spawns
    /// the task — frame 1 never stalls.
    ///
    /// Uses synthetic 8×8 sources (injectable via the
    /// `_with_sources` helper) so the background bake completes in
    /// milliseconds, and a temp userdata dir (via
    /// `HELIOS_USERDATA_DIR`) so the test never touches the real
    /// cache.
    #[test]
    fn cold_cache_bake_runs_async_not_inline() {
        // The async pool is a process global — init it if a prior
        // test/App didn't (get_or_init is idempotent).
        let _ = bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::new);

        // Redirect the cache dir to a throwaway temp directory.
        let tag = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp = std::env::temp_dir().join(format!("helios-icon-async-{tag}"));
        std::fs::create_dir_all(&temp).unwrap();
        std::env::set_var("HELIOS_USERDATA_DIR", &temp);
        std::fs::remove_dir_all(cache_dir()).ok();
        std::fs::create_dir_all(cache_dir()).unwrap();

        // Two tiny synthetic sources (8×8, dark-on-white so the
        // luminance key path is exercised).
        let src_a = temp.join("a.png");
        let src_b = temp.join("b.png");
        let make_icon = |path: &std::path::Path| {
            let img = image::RgbaImage::from_fn(8, 8, |x, y| {
                let v = if (x + y) % 2 == 0 { 20u8 } else { 240u8 };
                image::Rgba([v, v, v, 255])
            });
            img.save(path).expect("write test source");
        };
        make_icon(&src_a);
        make_icon(&src_b);
        let sources = HashMap::from([
            ("resource:Water".to_string(), src_a),
            ("energy".to_string(), src_b),
        ]);

        let mut icons = ResourceIcons::default();

        // First call: validation + task spawn only — NOT ready, and
        // the bake must be in flight (not done inline).
        bootstrap_icon_cache_with_sources(&mut icons, &sources);
        assert!(
            !icons.cache_ready,
            "cold cache must not flip ready in the first call (inline bake regression)"
        );
        assert!(
            icons.bake_task.is_some(),
            "cold cache must spawn the async bake task"
        );

        // Poll (with a small sleep for the background thread) until
        // the bake lands. Each call is non-blocking.
        let mut polls = 0usize;
        while !icons.cache_ready {
            polls += 1;
            std::thread::sleep(std::time::Duration::from_millis(10));
            bootstrap_icon_cache_with_sources(&mut icons, &sources);
            assert!(polls < 400, "async bake never finished after {polls} polls");
        }
        assert!(
            icons.bake_task.is_none(),
            "task handle not cleared on completion"
        );

        // Manifest was persisted with entries for both synthetic keys.
        let loaded = load_manifest(&cache_dir())
            .ok()
            .flatten()
            .expect("manifest written");
        assert!(
            loaded.entries.contains_key("resource:Water") && loaded.entries.contains_key("energy"),
            "manifest missing baked keys: {:?}",
            loaded.entries.keys().collect::<Vec<_>>()
        );
        // The version MUST be stamped — a `Default` manifest (v0)
        // fails validation and re-bakes every launch (the "49 stale
        // every time" bug found in smoke test 6).
        assert_eq!(
            loaded.version,
            icon_cache::CACHE_VERSION,
            "baked manifest must carry the schema version"
        );
        // Re-validate against the same sources → must be Fresh (no
        // re-bake on the next launch).
        let validation = validate(&cache_dir(), &Some(loaded.clone()), &sources, |p| {
            std::fs::read(p).ok().map(|b| fnv1a_hex(&b))
        });
        assert_eq!(
            validation,
            CacheValidation::Fresh,
            "warm cache must validate fresh (no re-bake)"
        );
        // Outputs cover every cache size AND are real files with
        // sanitized names (no `:` — illegal on Windows / ADS trap).
        let water = &loaded.entries["resource:Water"];
        assert_eq!(water.outputs.len(), icon_cache::ICON_CACHE_SIZES.len());
        assert!(
            !water.missing,
            "source existed — must not be marked missing"
        );
        for file_name in water.outputs.values() {
            assert!(
                !file_name.contains(':'),
                "output name must be sanitized, got {file_name}"
            );
            assert!(
                cache_dir().join(file_name).exists(),
                "baked output file missing: {file_name}"
            );
        }

        let _ = std::fs::remove_dir_all(&temp);
    }

    /// The boot overlay holds past `BootState::Ready` until every
    /// resource-bar icon the UI declared as needed has landed. This
    /// is the "no pop-in after the loading bar" contract (v0.5.2
    /// bugfix round 2): `all_needed_loaded` must be false while any
    /// needed icon is missing, and true once they're all present.
    #[test]
    fn all_needed_loaded_reflects_missing_icons() {
        let icons = ResourceIcons::default();
        let mut needs = ResourceIconNeeds::default();
        needs.mark_resource(ResourceType::Iron);
        needs.mark_category("Construction");
        needs.mark_energy();

        // Nothing loaded → not ready.
        assert!(!icons.all_needed_loaded(&needs));
        // A resource that isn't even needed must not block readiness.
        let needs_min = ResourceIconNeeds::default();
        assert!(icons.all_needed_loaded(&needs_min));

        // Marking a resource needed but not loaded keeps it unready.
        let mut needs_water = ResourceIconNeeds::default();
        needs_water.mark_resource(ResourceType::Water);
        assert!(!icons.all_needed_loaded(&needs_water));

        // Energy needed but not loaded → unready.
        let mut needs_energy = ResourceIconNeeds::default();
        needs_energy.mark_energy();
        assert!(!icons.all_needed_loaded(&needs_energy));
    }
}
