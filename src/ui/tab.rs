//! The `Tab` trait — the shared vocabulary for in-panel sub-tab strips
//! (Pattern 4 in `docs/UI_LAYOUT_PATTERNS.md`).
//!
//! Every panel that has a *fixed* set of top-level sub-tabs
//! (Construction, Research, Economy, and — via the Bevy UI mirror —
//! Shipbuilding) can re-express its `*Tab` enum as one primitive by
//! implementing this trait. `theme::tab_strip<T: Tab>(...)` and
//! `theme::tab_strip_bevy<T: Tab>(...)` then render the strip without
//! bespoke per-panel `draw_tab_button` calls.
//!
//! The trait is intentionally minimal: a stable ID, a label, an optional
//! icon. Panels that need dynamic labels (e.g. Construction's
//! `🛠 Build (1.4 BP/yr)`) can still iterate the enum themselves — the
//! `tab_strip` primitive is a building block, not a hard mandate.
//!
//! **Convention.** `Default` is the canonical first tab; `tab_strip`
//! uses it as the default `active` argument so callers writing
//! `tab_strip(ui, &MyTabs::all(), MyTabs::default(), on_select)` get the
//! expected behaviour.

use std::borrow::Cow;

/// A single sub-tab in a panel's tab strip.
///
/// Panels implement this trait on their `*Tab` enum. The trait is
/// `Copy`-bound via `Self: Copy` so the egui primitive can move
/// instances through its callback chain without cloning; every
/// current `*Tab` enum is `Copy` already (they're plain fieldless
/// variants). `PartialEq` lets the primitive compare `*tab` against
/// the `active` argument to pick the active styling.
#[allow(dead_code)]
pub trait Tab: Copy + PartialEq {
    /// Stable identifier for egui's `Id` system. Two panels must not
    /// share an `id` if their strips could ever be visible at the same
    /// time — egui will reuse the response state otherwise. Convention:
    /// the enum variant's snake_case name (e.g. `"overview"`,
    /// `"buildings"`).
    #[allow(dead_code)]
    fn id(&self) -> &'static str;

    /// Human-readable label rendered inside the tab button. The trait
    /// exposes it as `Cow<'static, str>` so panels can return static
    /// literals (`Cow::Borrowed("Overview")`) today and a `format!`ed
    /// string (`Cow::Owned(format!("Build ({rate:.1} BP/yr)", rate))`)
    /// later without breaking the signature.
    fn label(&self) -> Cow<'static, str>;

    /// Optional leading icon glyph (emoji or symbol). `None` means
    /// the label is rendered without a prefix.
    fn icon(&self) -> Option<&'static str> {
        None
    }
}

#[cfg(test)]
mod tests {
    //! Compile-time + runtime sanity for the `Tab` trait and the
    //! `theme::tab_strip<T>` primitive. The egui primitive itself
    //! can't be exercised headlessly without an `egui::Context` (and a
    //! full Bevy app loop), so this module focuses on what we *can*
    //! prove: the trait's contract, the dynamic-label `Cow::Owned`
    //! extension point, and the `Default`-first convention used by
    //! `tab_strip`'s default-arg call sites.
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    enum DemoTab {
        #[default]
        One,
        Two,
        Three,
    }

    impl Tab for DemoTab {
        fn id(&self) -> &'static str {
            match self {
                Self::One => "one",
                Self::Two => "two",
                Self::Three => "three",
            }
        }

        fn label(&self) -> Cow<'static, str> {
            Cow::Borrowed(match self {
                Self::One => "One",
                Self::Two => "Two",
                Self::Three => "Three",
            })
        }

        fn icon(&self) -> Option<&'static str> {
            match self {
                Self::One => Some("①"),
                Self::Two => Some("②"),
                Self::Three => None,
            }
        }
    }

    #[test]
    fn tab_trait_returns_expected_ids_labels_icons() {
        assert_eq!(DemoTab::One.id(), "one");
        assert_eq!(DemoTab::One.label(), "One");
        assert_eq!(DemoTab::One.icon(), Some("①"));

        assert_eq!(DemoTab::Two.id(), "two");
        assert_eq!(DemoTab::Two.icon(), Some("②"));

        assert_eq!(DemoTab::Three.id(), "three");
        assert_eq!(DemoTab::Three.icon(), None);
    }

    #[test]
    fn tab_trait_supports_dynamic_owned_labels() {
        // The `Cow<'static, str>` return type lets a future panel
        // override `label` to return `Cow::Owned(format!(...))` for
        // dynamic labels (e.g. Construction's BP rate) without
        // changing the trait signature. This test exercises that
        // pattern via a free helper that mirrors what a panel would
        // do.
        fn label_with_suffix(t: &DemoTab, suffix: &str) -> Cow<'static, str> {
            if *t == DemoTab::Two {
                Cow::Owned(format!("Two ({suffix})"))
            } else {
                t.label()
            }
        }

        assert_eq!(label_with_suffix(&DemoTab::One, "x"), "One");
        assert_eq!(label_with_suffix(&DemoTab::Two, "x"), "Two (x)");
        assert_eq!(label_with_suffix(&DemoTab::Three, "ignored"), "Three");
    }

    #[test]
    fn default_is_the_canonical_first_tab() {
        // `tab_strip`'s `Default` convention means callers writing
        // `tab_strip(ui, &tabs, MyTabs::default(), on_select)` get
        // the expected first-tab behaviour. This is the same
        // invariant every existing `*Tab` enum upholds.
        assert_eq!(DemoTab::default(), DemoTab::One);
    }

    /// Compile-time check that `theme::tab_strip<T>` and
    /// `theme::ledger_panel<T>` accept a `Tab` impl. The body of this
    /// function is never executed — if it compiles, the primitive
    /// signatures are usable by a panel that implements `Tab` on
    /// its `*Tab` enum.
    fn _tab_primitives_compile(ui: &mut bevy_egui::egui::Ui) {
        let tabs = [DemoTab::One, DemoTab::Two, DemoTab::Three];
        let _ = super::super::theme::tab_strip(ui, &tabs, DemoTab::One, |_t| {});
        super::super::theme::ledger_panel(ui, "dossier_demo", "DEMO", &(), |_ui| {});
    }
}
