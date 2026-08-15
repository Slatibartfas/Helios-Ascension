//! Post-kickoff boot progress overlay (native Bevy UI).
//!
//! ## Why (v0.5.2, 2026-08-05)
//!
//! The splash screen shows an indeterminate spinner (see
//! `src/ui/launch/splash.rs`), because during the splash the
//! boot-init chain is gated on `WorldReady` — which only exists
//! after the player clicks New Game / Continue / Load (GRA-358
//! PR-B, see `src/persistence/swap.rs`). So the splash has no
//! real progress to report.
//!
//! The real work happens AFTER the click: the 15-step boot chain
//! (`BootInitPlugin` in `src/boot_init.rs`) spawns the solar
//! system, nearby systems, resources, fleets and tech, one step
//! per `Update` frame. Before this overlay existed the player saw
//! a **blank main window** for those 15 frames — no feedback that
//! anything was happening.
//!
//! This overlay is a native Bevy UI panel (not egui) so it can run
//! in `Update` alongside the chain, gated on:
//! - `LaunchState::InGame` (the player committed to a session)
//! - `BootState::Loading` (the chain hasn't finished)
//!
//! It shows `Generating world… N/15` + a cyan progress bar. When
//! `mark_boot_ready` flips `BootState → Ready`, the overlay hides
//! itself on the next frame.
//!
//! ## Why native UI, not egui
//!
//! The boot chain runs in `Update`. bevy_egui's `EguiPrimaryContextPass`
//! is a separate pass that may not be ready on the first boot frames
//! (and the pre-parse + chain are deliberately kept off the egui
//! pass). A native `Node` spawn in `Update` + a per-frame text/width
//! rewrite is the same pattern the construction canary uses, and it
//! has no egui-context ordering dependency.

use bevy::prelude::*;

use crate::boot_init::{BootProgress, BootState};
use crate::ui::bevy_theme::CYAN;
use crate::ui::launch::LaunchState;

/// Marker on the overlay root.
#[derive(Component)]
pub struct BootOverlayRoot;

/// Marker on the progress text node (rewritten per frame).
#[derive(Component)]
pub struct BootOverlayText;

/// Marker on the progress-fill bar (width rewritten per frame).
#[derive(Component)]
pub struct BootOverlayFill;

/// Spawn the overlay once (self-gated by marker presence). Runs in
/// `Update`; the overlay is a full-screen OPAQUE backdrop with a
/// centered card.
///
/// ## Why opaque (v0.5.2, 2026-08-05)
///
/// The boot chain spawns bodies one step per frame, so during
/// generation the 3D world pops in piece by piece behind the
/// overlay. A translucent backdrop (55 % alpha originally) let that
/// pop-in show through — the "nasty pop-up of assets one by one"
/// the player reported. An opaque backdrop fully hides the world
/// until `mark_boot_ready` flips `BootState → Ready`, at which point
/// the overlay hides and the fully-populated game appears in one
/// clean transition.
fn spawn_boot_overlay(mut commands: Commands, existing: Query<(), With<BootOverlayRoot>>) {
    if !existing.is_empty() {
        return;
    }
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            // Opaque — the boot chain spawns the world one step per
            // frame and the pop-in must stay hidden until Ready.
            BackgroundColor(Color::srgba(0.008, 0.016, 0.031, 1.0)),
            GlobalZIndex(200),
            Visibility::Hidden,
            BootOverlayRoot,
            Name::new("boot_overlay"),
        ))
        .id();

    let card = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(12.0),
                padding: UiRect::all(Val::Px(24.0)),
                width: Val::Px(360.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.008, 0.039, 0.094, 0.96)),
            BorderColor::all(CYAN),
            BootOverlayCard,
            Name::new("boot_overlay_card"),
        ))
        .id();
    commands.entity(root).add_child(card);

    // Progress text: "Generating world… N/15"
    let text = commands
        .spawn((
            Text::new("Generating world…"),
            TextFont {
                font_size: 18.0,
                ..default()
            },
            TextColor(CYAN),
            BootOverlayText,
            Name::new("boot_overlay_text"),
        ))
        .id();
    commands.entity(card).add_child(text);

    // Track + fill bar.
    let track = commands
        .spawn((
            Node {
                width: Val::Px(280.0),
                height: Val::Px(8.0),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.086, 0.188, 0.306, 0.6)),
            Name::new("boot_overlay_track"),
        ))
        .id();
    commands.entity(card).add_child(track);

    let fill = commands
        .spawn((
            Node {
                width: Val::Px(0.0),
                height: Val::Px(8.0),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(CYAN),
            BootOverlayFill,
            Name::new("boot_overlay_fill"),
        ))
        .id();
    commands.entity(track).add_child(fill);
    // NOTE: do NOT insert `Children::default()` on the root here —
    // `add_child` already maintains the root's `Children` component.
    // Wiping it with an empty `Children` orphans the card from the
    // root's hierarchy: the card keeps `ChildOf(root)` but the root
    // no longer lists it, so it renders detached at the top-left
    // with default visibility (the "Generating world" box stuck in
    // the corner of the main menu, immune to the root's
    // `Visibility::Hidden` — the 2026-08-05 regression).
}

/// Marker for the inner card (needed only to keep the hierarchy
/// readable; the visibility logic targets the root).
#[derive(Component)]
pub struct BootOverlayCard;

/// Per-frame visibility + text/width update. Shows only when
/// `LaunchState::InGame && BootState::Loading`, and holds past
/// `Ready` until every resource-bar icon has loaded (so the icons
/// never pop in one-by-one after the progress bar finishes —
/// v0.5.2 bugfix round 2).
fn update_boot_overlay(
    mut root_query: Query<&mut Visibility, (With<BootOverlayRoot>, Without<BootOverlayText>)>,
    mut text_query: Query<&mut Text, With<BootOverlayText>>,
    mut fill_query: Query<&mut Node, With<BootOverlayFill>>,
    launch_state: Res<LaunchState>,
    boot_state: Res<BootState>,
    boot_progress: Option<Res<BootProgress>>,
    icons: Option<Res<crate::ui::resource_icons::ResourceIcons>>,
    needs: Option<Res<crate::ui::resource_icons::ResourceIconNeeds>>,
    real_time: Res<Time<Real>>,
    // v0.5.2 (2026-08-06): the timestamp (real seconds) at which the
    // chain reached `Ready` while icons were still missing. When this
    // exceeds `MAX_FINALIZING_S`, force-hide the overlay — a stuck
    // icon (missing file, cache bake failure) must NEVER permanently
    // block the player behind the opaque backdrop. Icons popping in
    // late is far better than an unreachable menu.
    mut finalizing_started: Local<Option<f64>>,
) {
    let in_game = launch_state.is_in_game();
    let loading = *boot_state == BootState::Loading;
    // Icons are "ready" when the resources bar has declared needs and
    // they've all landed. If the icon systems aren't registered (a
    // test App without UIPlugin), treat as ready so the overlay never
    // deadlocks on a missing resource.
    let icons_ready = match (icons, needs) {
        (Some(icons), Some(needs)) => icons.all_needed_loaded(&needs),
        _ => true,
    };

    // v0.5.2 (2026-08-06): hard fail-safe. If the chain is Ready but
    // icons are still missing, start a timer. Once it exceeds
    // `MAX_FINALIZING_S`, treat icons as ready (hide the overlay) so
    // a single missing icon can't lock the game. The timer resets
    // when the overlay isn't in the Finalizing state, so a legit slow
    // load that completes keeps its full grace period.
    const MAX_FINALIZING_S: f64 = 5.0;
    let now = real_time.elapsed_secs_f64();
    let mut icons_ready = icons_ready;
    if !loading && !icons_ready {
        let started = *finalizing_started.get_or_insert(now);
        if now - started >= MAX_FINALIZING_S {
            warn!(
                "boot overlay: force-dismissing after {MAX_FINALIZING_S:.0}s (icons not all \
                 loaded — check resource_icons.rs for a missing file); menus may show \
                 placeholder icons"
            );
            icons_ready = true;
        }
    } else {
        *finalizing_started = None;
    }

    // Visible while the chain runs OR while the icons are still
    // trickling in — the opaque backdrop hides the pop-in until both
    // are done.
    let visible = in_game && (loading || !icons_ready);
    for mut vis in root_query.iter_mut() {
        *vis = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if !visible {
        return;
    }

    // "Generating world… N/15" — but once the chain is Ready (icons
    // still loading), say "Finalizing…" so the bar reads honestly.
    if let (Ok(mut text), Some(progress)) = (text_query.single_mut(), boot_progress.as_ref()) {
        if loading {
            let shown = (progress.step).saturating_add(1).min(progress.total);
            let total = progress.total.max(1);
            text.0 = format!("Generating world… {shown}/{total}");
        } else {
            text.0 = "Finalizing…".to_string();
        }
    }
    // Fill width = step/total * 280px (full when Ready).
    if let (Ok(mut node), Some(progress)) = (fill_query.single_mut(), boot_progress.as_ref()) {
        let frac = (progress.step as f32 + 1.0) / (progress.total.max(1) as f32);
        node.width = Val::Px(280.0 * frac.clamp(0.0, 1.0));
    }
}

/// Plugin that owns the boot overlay lifecycle.
pub struct BootOverlayPlugin;

impl Plugin for BootOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_boot_overlay, update_boot_overlay));
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
    use crate::boot_init::BOOT_STEP_COUNT;

    #[test]
    fn boot_step_count_is_still_fifteen() {
        // The overlay renders `N/{total}` from `BootProgress`; if
        // the chain grows, `BOOT_STEP_COUNT` changes and this test
        // guards the overlay's assumption (it just reads the
        // resource, so the test is really a canary that the
        // constant stays consistent with the step table).
        assert_eq!(BOOT_STEP_COUNT, 15);
    }

    #[test]
    fn progress_mapping_clamps() {
        // step 0 → "1/15", step 14 → "15/15", never 0/15 or 16/15.
        let total = 15u32;
        for step in 0..15u32 {
            let shown = (step).saturating_add(1).min(total);
            assert!(shown >= 1 && shown <= total);
        }
        let shown = (14u32).saturating_add(1).min(total);
        assert_eq!(shown, 15);
        let shown = (0u32).saturating_add(1).min(total);
        assert_eq!(shown, 1);
    }
}
