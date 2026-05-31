//! UI Animation infrastructure for Helios Ascension.
//!
//! Provides non-blocking animation helpers for:
//! - Panel fade-in/out transitions
//! - Tab slide transitions
//! - Button press feedback
//! - Progress bar smooth fill
//! - Toast notifications
//! - Resource delta popups

#![allow(dead_code)]

use bevy::prelude::*;
use bevy_egui::egui;
use std::collections::HashMap;

// ─── Timing Constants ─────────────────────────────────────────────────────────

/// Panel fade-in duration (seconds)
pub const PANEL_FADE_IN: f32 = 0.200;
/// Panel fade-out duration (seconds)
pub const PANEL_FADE_OUT: f32 = 0.150;
/// Tab slide transition duration (seconds)
pub const TAB_SLIDE: f32 = 0.150;
/// Button press animation duration (seconds)
pub const BUTTON_PRESS: f32 = 0.080;
/// Progress bar smooth fill duration (seconds)
pub const PROGRESS_FILL: f32 = 0.300;
/// Toast slide-in duration (seconds)
pub const TOAST_SLIDE_IN: f32 = 0.250;
/// Toast slide-out duration (seconds)
pub const TOAST_SLIDE_OUT: f32 = 0.200;
/// Toast auto-dismiss duration (seconds)
pub const TOAST_DURATION: f32 = 3.0;
/// Resource delta popup total lifetime (seconds)
pub const DELTA_POPUP_LIFETIME: f32 = 1.2;
/// Resource delta float-up duration (seconds)
pub const DELTA_FLOAT_UP: f32 = 0.8;

// ─── Easing Helpers ───────────────────────────────────────────────────────────

/// Linear interpolation
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Ease-out cubic: fast start, slow end
pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// Ease-in cubic: slow start, fast end
pub fn ease_in(t: f32) -> f32 {
    t.clamp(0.0, 1.0).powi(3)
}

/// Ease-in-out cubic: slow start and end, fast middle
pub fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

// ─── Panel Fade Animation ─────────────────────────────────────────────────────

/// Per-panel fade state for open/close transitions.
#[derive(Component, Clone)]
pub struct PanelFade {
    /// Current alpha (0.0 = invisible, 1.0 = fully visible)
    pub alpha: f32,
    /// True if panel is currently open
    pub open: bool,
    /// True once panel has reached full visibility (no need to animate further)
    pub fully_open: bool,
    /// True once panel has finished fading out
    pub fully_closed: bool,
}

impl PanelFade {
    pub fn new() -> Self {
        Self {
            alpha: 0.0,
            open: false,
            fully_open: false,
            fully_closed: true,
        }
    }

    /// Call each frame with `dt` to advance the fade animation.
    /// Returns the current alpha for rendering.
    pub fn update(&mut self, dt: f32) -> f32 {
        if self.fully_open && self.open {
            return 1.0;
        }
        if self.fully_closed && !self.open {
            return 0.0;
        }

        if self.open && !self.fully_open {
            // Fade in
            let speed = 1.0 / PANEL_FADE_IN;
            self.alpha = (self.alpha + dt * speed).min(1.0);
            if self.alpha >= 1.0 {
                self.alpha = 1.0;
                self.fully_open = true;
            }
        } else if !self.open && !self.fully_closed {
            // Fade out
            let speed = 1.0 / PANEL_FADE_OUT;
            self.alpha = (self.alpha - dt * speed).max(0.0);
            if self.alpha <= 0.0 {
                self.alpha = 0.0;
                self.fully_closed = true;
            }
        }
        self.alpha
    }

    /// Begin opening the panel (triggers fade-in)
    pub fn open_panel(&mut self) {
        self.open = true;
        self.fully_open = false;
        self.fully_closed = false;
    }

    /// Begin closing the panel (triggers fade-out)
    pub fn close_panel(&mut self) {
        self.open = false;
        self.fully_closed = false;
        self.fully_open = false;
    }
}

impl Default for PanelFade {
    fn default() -> Self {
        Self::new()
    }
}

/// Manager for per-panel fade states (used as a Bevy Resource).
/// Tracks fade animations for multiple named panels.
#[derive(Resource, Default)]
pub struct PanelFades {
    pub fades: std::collections::HashMap<String, PanelFade>,
}

impl PanelFades {
    pub fn new() -> Self {
        Self {
            fades: std::collections::HashMap::new(),
        }
    }

    /// Get or create a fade state for a panel.
    pub fn get_or_insert(&mut self, name: &str) -> &mut PanelFade {
        self.fades.entry(name.to_string()).or_insert_with(PanelFade::new)
    }

    /// Update all panel fade animations. Returns alpha for named panel (if exists).
    pub fn update_all(&mut self, dt: f32) {
        for fade in self.fades.values_mut() {
            fade.update(dt);
        }
    }

    /// Trigger fade-in for a panel (creates if not exists).
    pub fn trigger_fade_in(&mut self, name: &str) {
        let fade = self.get_or_insert(name);
        fade.open_panel();
    }

    /// Trigger fade-out for a panel.
    pub fn trigger_fade_out(&mut self, name: &str) {
        if let Some(fade) = self.fades.get_mut(name) {
            fade.close_panel();
        }
    }

    /// Get current alpha for a panel (0.0 if not tracked).
    pub fn alpha(&self, name: &str) -> f32 {
        self.fades.get(name).map(|f| f.alpha).unwrap_or(1.0)
    }
}

/// Apply alpha to a color for panel fade effect.
pub fn alpha_multiply(color: egui::Color32, alpha: f32) -> egui::Color32 {
    egui::Color32::from_rgba_premultiplied(
        color.r(),
        color.g(),
        color.b(),
        (color.a() as f32 * alpha) as u8,
    )
}

// ─── Tab Slide Animation ──────────────────────────────────────────────────────

/// Per-tab-group state for slide transitions.
#[derive(Component, Clone, Default)]
pub struct TabSlide {
    /// Current horizontal offset (pixels)
    pub offset: f32,
    /// Target offset (pixels) — set when tab changes
    pub target_offset: f32,
    /// True when animating
    pub animating: bool,
}

impl TabSlide {
    pub fn new() -> Self {
        Self::default()
    }

    /// Trigger a new tab slide animation.
    /// `target` is the new tab's target offset (typically 0.0).
    pub fn slide_to(&mut self, target: f32, panel_width: f32) {
        // Set target = incoming tab position, current becomes outgoing
        self.offset = panel_width; // Start from off-screen right
        self.target_offset = target;
        self.animating = true;
    }

    /// Update each frame with `dt`. Returns current offset.
    pub fn update(&mut self, dt: f32) -> f32 {
        if !self.animating {
            return self.offset;
        }

        let speed = self.target_offset.abs() / TAB_SLIDE;
        let delta = (self.target_offset - self.offset).signum() * speed * dt;

        if delta.abs() < (self.target_offset - self.offset).abs() {
            self.offset += delta;
        } else {
            self.offset = self.target_offset;
            self.animating = false;
        }

        self.offset
    }

    /// Immediate jump to target (no animation)
    pub fn snap_to(&mut self, target: f32) {
        self.offset = target;
        self.target_offset = target;
        self.animating = false;
    }
}

// ─── Smooth Progress Bar ──────────────────────────────────────────────────────

/// State for smooth progress bar animations.
/// Tracks displayed value vs actual value for lerping.
#[derive(Component, Clone, Default)]
pub struct SmoothProgress {
    /// Currently displayed progress (0.0 to 1.0)
    pub displayed: f32,
    /// Target progress (updated from game logic)
    pub target: f32,
}

impl SmoothProgress {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a new target value (call each frame game value changes)
    pub fn set_target(&mut self, target: f32) {
        self.target = target.clamp(0.0, 1.0);
    }

    /// Update the displayed value toward target. Call each frame.
    pub fn update(&mut self, dt: f32) -> f32 {
        let speed = 1.0 / PROGRESS_FILL;
        let diff = self.target - self.displayed;
        let step = speed * dt;

        if diff.abs() < step {
            self.displayed = self.target;
        } else {
            self.displayed += diff.signum() * step;
        }
        self.displayed
    }
}

/// Render a smooth progress bar that animates from current to target value.
/// Call each frame with the current displayed value and the actual target.
/// Returns the new displayed value for tracking.
pub fn smooth_progress_bar_ui(
    ui: &mut egui::Ui,
    current_displayed: f32,
    target: f32,
) -> f32 {
    let dt = ui.ctx().input(|i| i.global_time().dt_in_seconds());
    let speed = 1.0 / PROGRESS_FILL;
    let diff = target - current_displayed;
    let step = speed * dt;

    let new_displayed = if diff.abs() < step {
        target
    } else {
        current_displayed + diff.signum() * step
    };

    ui.add(egui::ProgressBar::new(new_displayed));
    new_displayed
}

// ─── Toast Notifications ──────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastKind {
    pub fn color(&self) -> egui::Color32 {
        match self {
            ToastKind::Info => egui::Color32::from_rgb(100, 180, 255),
            ToastKind::Success => egui::Color32::from_rgb(80, 220, 120),
            ToastKind::Warning => crate::ui::theme::AMBER,
            ToastKind::Error => crate::ui::theme::RED,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            ToastKind::Info => "\u{2139}",     // ℹ
            ToastKind::Success => "\u{2713}",  // ✓
            ToastKind::Warning => "\u{26A0}",   // ⚠
            ToastKind::Error => "\u{2717}",     // ✗
        }
    }
}

#[derive(Clone)]
pub struct ToastMessage {
    pub text: String,
    pub kind: ToastKind,
    pub start_time: f64,
    pub duration_secs: f32,
    /// Slide-in animation progress (0.0 = off-screen, 1.0 = visible)
    pub slide_in: f32,
    /// Slide-out animation progress (0.0 = visible, 1.0 = off-screen)
    pub slide_out: f32,
    /// Whether we've started the slide-out phase
    pub dismissing: bool,
}

impl ToastMessage {
    pub fn new(text: impl Into<String>, kind: ToastKind) -> Self {
        Self {
            text: text.into(),
            kind,
            start_time: 0.0,
            duration_secs: TOAST_DURATION,
            slide_in: 0.0,
            slide_out: 0.0,
            dismissing: false,
        }
    }

    pub fn with_duration(mut self, duration_secs: f32) -> Self {
        self.duration_secs = duration_secs;
        self
    }

    /// Elapsed time since toast was created
    pub fn elapsed(&self, time: f64) -> f32 {
        (time - self.start_time) as f32
    }

    /// Total visible time before auto-dismiss
    pub fn total_lifetime(&self) -> f32 {
        self.duration_secs + TOAST_SLIDE_IN + TOAST_SLIDE_OUT
    }

    /// Update animation state. Returns true if toast should be removed.
    pub fn update(&mut self, _dt: f32, time: f64) -> bool {
        let elapsed = self.elapsed(time);

        // Slide in phase (first TOAST_SLIDE_IN seconds)
        if !self.dismissing {
            let slide_in_duration = TOAST_SLIDE_IN;
            self.slide_in = ease_out((elapsed / slide_in_duration).min(1.0));
        }

        // Check if we should start dismissing
        if !self.dismissing && elapsed >= self.duration_secs {
            self.dismissing = true;
        }

        // Slide out phase
        if self.dismissing {
            let dismiss_elapsed = elapsed - self.duration_secs;
            let slide_out_duration = TOAST_SLIDE_OUT;
            self.slide_out = ease_in((dismiss_elapsed / slide_out_duration).min(1.0));
        }

        // Remove when fully slid out
        elapsed >= self.total_lifetime()
    }
}

#[derive(Resource)]
pub struct ToastQueue {
    pub toasts: Vec<ToastMessage>,
    pub max_visible: usize,
    /// Last known egui time, used by helper methods as a fallback when
    /// no egui context is available.
    last_time: f64,
}

impl Default for ToastQueue {
    fn default() -> Self {
        Self {
            toasts: Vec::new(),
            max_visible: 5,
            last_time: 0.0,
        }
    }
}

impl ToastQueue {
    /// Add a toast message to the queue
    pub fn push(&mut self, toast: ToastMessage) {
        self.toasts.push(toast);
        // Limit visible toasts
        while self.toasts.len() > self.max_visible {
            self.toasts.remove(0);
        }
    }

    /// Add an info toast
    pub fn info(&mut self, text: impl Into<String>) {
        let mut toast = ToastMessage::new(text, ToastKind::Info);
        toast.start_time = self.last_time;
        self.push(toast);
    }

    /// Add a success toast
    pub fn success(&mut self, text: impl Into<String>) {
        let mut toast = ToastMessage::new(text, ToastKind::Success);
        toast.start_time = self.last_time;
        self.push(toast);
    }

    /// Add a warning toast
    pub fn warn(&mut self, text: impl Into<String>) {
        let mut toast = ToastMessage::new(text, ToastKind::Warning);
        toast.start_time = self.last_time;
        self.push(toast);
    }

    /// Add an error toast
    pub fn error(&mut self, text: impl Into<String>) {
        let mut toast = ToastMessage::new(text, ToastKind::Error);
        toast.start_time = self.last_time;
        self.push(toast);
    }

    /// Update all toasts. Removes expired ones.
    /// Must be called from an egui context (e.g. inside a bevy_egui system that has `ctx: &egui::Context`).
    pub fn update(&mut self, dt: f32, time: f64) {
        self.last_time = time;
        let mut survivors = Vec::new();
        for t in &mut self.toasts {
            if !t.update(dt, time) {
                survivors.push(t.clone());
            }
        }
        self.toasts = survivors;
    }
}

/// Render toast notifications using egui.
pub fn render_toasts(ctx: &egui::Context, toasts: &[ToastMessage]) {
    if toasts.is_empty() {
        return;
    }

    let available_rect = ctx.available_rect();
    let toast_width = 280.0;
    let toast_height = 60.0;
    let spacing = 8.0;

    // Stack from bottom-right, newest on top
    let mut y_offset = 0.0;

    for toast in toasts.iter().rev() {
        // Calculate x position based on slide animation
        let slide_progress = if toast.dismissing {
            toast.slide_out
        } else {
            1.0 - toast.slide_in
        };

        // x = off-screen right when slide=0, visible when slide=1
        let base_x = available_rect.right();
        let target_x = available_rect.right() - toast_width;
        let x = lerp(base_x, target_x, 1.0 - slide_progress);
        let y = available_rect.bottom() - toast_height - y_offset;

        let pos = egui::pos2(x, y);

        let frame = egui::Frame::popup(ctx.style().as_ref())
            .fill(egui::Color32::from_rgba_unmultiplied(12, 16, 28, 250))
            .stroke(egui::Stroke::new(1.5, toast.kind.color()));

        egui::Area::new(egui::Id::new(format!("toast_{}", pos.x as i32)))
            .fixed_pos(pos)
            .interactable(false)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                frame.show(ui, |ui| {
                    ui.set_min_width(toast_width);
                    ui.set_max_width(toast_width);
                    ui.set_height(toast_height);

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(toast.kind.icon())
                                .size(18.0)
                                .color(toast.kind.color()),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(&toast.text)
                                .size(13.0)
                                .color(egui::Color32::WHITE),
                        );
                    });
                });
            });

        y_offset += toast_height + spacing;
    }
}

// ─── Resource Delta Popups ────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ResourceDelta {
    pub resource_name: String,
    pub delta: f64,
    pub start_x: f32,
    pub start_y: f32,
    pub start_time: f64,
}

impl ResourceDelta {
    pub fn new(resource_name: impl Into<String>, delta: f64, x: f32, y: f32) -> Self {
        Self {
            resource_name: resource_name.into(),
            delta,
            start_x: x,
            start_y: y,
            start_time: 0.0,
        }
    }

    /// Elapsed time since popup creation
    pub fn elapsed(&self, time: f64) -> f32 {
        (time - self.start_time) as f32
    }

    /// Update and check if expired. Returns (current_x, current_y, alpha).
    pub fn update(&mut self, _dt: f32, time: f64) -> Option<(f32, f32, f32)> {
        let elapsed = self.elapsed(time);

        if elapsed >= DELTA_POPUP_LIFETIME {
            return None;
        }

        // Float upward
        let float_progress = (elapsed / DELTA_FLOAT_UP).min(1.0);
        let y = self.start_y - (40.0 * ease_out(float_progress));
        let x = self.start_x;

        // Fade out over last 400ms
        let fade_start = DELTA_POPUP_LIFETIME - 0.4;
        let alpha = if elapsed > fade_start {
            1.0 - ((elapsed - fade_start) / 0.4).min(1.0)
        } else {
            1.0
        };

        Some((x, y, alpha))
    }

    /// Formatted string for display
    pub fn formatted(&self) -> String {
        let sign = if self.delta >= 0.0 { "+" } else { "" };
        let abs_val = self.delta.abs();
        let formatted = if abs_val >= 1_000_000.0 {
            format!("{:.1}M", abs_val / 1_000_000.0)
        } else if abs_val >= 1_000.0 {
            format!("{:.1}k", abs_val / 1_000.0)
        } else {
            format!("{:.0}", abs_val)
        };
        format!("{}{} {}", sign, formatted, self.resource_name)
    }

    pub fn is_positive(&self) -> bool {
        self.delta >= 0.0
    }
}

#[derive(Resource, Default)]
pub struct ResourceDeltaQueue {
    pub deltas: Vec<ResourceDelta>,
    pub max_deltas: usize,
}

impl ResourceDeltaQueue {
    pub fn new() -> Self {
        Self {
            deltas: Vec::new(),
            max_deltas: 10,
        }
    }

    /// Add a new delta popup.
    /// `time` should be the current egui time (from `ctx.input(|i| i.time)`).
    pub fn push(&mut self, mut delta: ResourceDelta, time: f64) {
        delta.start_time = time;
        self.deltas.push(delta);
        while self.deltas.len() > self.max_deltas {
            self.deltas.remove(0);
        }
    }

    /// Update all deltas. Removes expired ones.
    /// Must be called from an egui context (e.g. inside a bevy_egui system that has `ctx: &egui::Context`).
    pub fn update(&mut self, dt: f32, time: f64) {
        let mut survivors = Vec::new();
        for d in &mut self.deltas {
            if d.update(dt, time).is_some() {
                survivors.push(d.clone());
            }
        }
        self.deltas = survivors;
    }
}

/// Render resource delta popups using egui.
pub fn render_resource_deltas(ctx: &egui::Context, deltas: &[ResourceDelta], dt: f64) {
    if deltas.is_empty() {
        return;
    }

    let time = ctx.input(|i| i.time);
    for delta in deltas.iter() {
        if let Some((x, y, alpha)) = delta.clone().update(dt as f32, time) {
            let color = if delta.is_positive() {
                crate::ui::theme::GREEN
            } else {
                crate::ui::theme::RED
            };

            let text = delta.formatted();
            let galley = ctx.fonts(|f| {
                f.layout_no_wrap(
                    text,
                    egui::FontId::new(14.0, egui::FontFamily::Monospace),
                    color.linear_multiply(alpha),
                )
            });

            // Position centered above the starting point
            let pos = egui::pos2(x - galley.size().x / 2.0, y);

            egui::Area::new(egui::Id::new(format!("delta_{}_{}", x, y)))
                .fixed_pos(pos)
                .interactable(false)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(&text)
                            .font(egui::FontId::new(14.0, egui::FontFamily::Monospace))
                            .color(color.linear_multiply(alpha)),
                    );
                });
        }
    }
}

// ─── Button Press Animation Helper ───────────────────────────────────────────

/// Widget state for button animation
#[derive(Clone, Copy, PartialEq)]
pub enum ButtonAnimState {
    Normal,
    Hovered,
    Pressed,
}

/// Returns the visual properties for a button in the given state.
/// Used by theme.rs for consistent button styling.
pub fn button_anim_fill(state: ButtonAnimState) -> egui::Color32 {
    match state {
        ButtonAnimState::Normal => crate::ui::theme::SURFACE,
        ButtonAnimState::Hovered => crate::ui::theme::SURFACE_RAISED,
        ButtonAnimState::Pressed => egui::Color32::from_rgb(0, 80, 90),
    }
}

pub fn button_anim_stroke(state: ButtonAnimState) -> egui::Stroke {
    match state {
        ButtonAnimState::Normal => egui::Stroke::new(0.5, crate::ui::theme::BORDER),
        ButtonAnimState::Hovered => egui::Stroke::new(1.0, crate::ui::theme::ACCENT_DIM),
        ButtonAnimState::Pressed => egui::Stroke::new(1.5, crate::ui::theme::ACCENT),
    }
}
