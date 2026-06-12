//! Resource visibility tiers — the bridge between the v0.5.0 survey
//! state and the dossier/economy/dashboard UI.
//!
//! The legacy `discovered_amount(SurveyLevel)` function (in
//! `crate::economy::components`) returns a single fixed slice of a
//! deposit's reserve per legacy survey level. The v0.5.0 rework
//! replaces that with a tier-aware estimate that returns a
//! `(low, mid, high)` triplet plus the visibility class. The UI
//! renders the triplet directly, so the player sees the uncertainty.
//!
//! Tier semantics for `MineralDeposits` dimension (per
//! SURVEY_REWORK.md §11):
//!
//! | Tier | What the player sees                              | Reserve slice |
//! |------|---------------------------------------------------|---------------|
//! | 0    | Unknown (no row)                                  | none          |
//! | 1    | Class only ("Iron")                               | none          |
//! | 2    | Class + low range (proven_crustal, wide band)     | proven        |
//! | 3    | Class + mid range (proven + deep, medium band)    | proven + deep |
//! | 4    | Class + narrow range (full reserve, narrow band)  | full          |
//! | 5    | Precise (single number)                           | full          |
//!
//! The band width is widened by `(1.0 - confidence)` so that
//! stale data shows a wider interval than fresh data at the same
//! tier. The `SurveyState` from PR-A is the source of truth; the
//! legacy `SurveyLevel` is a 1:1 adapter via
//! [`crate::economy::components::SurveyLevel::as_deposit_fidelity`].

use serde::{Deserialize, Serialize};

use super::components::DimensionFidelity;
use super::types::{MAX_TIER, WARNING_CONFIDENCE};
use crate::economy::components::{MineralDeposit, ResourceReserve, SurveyLevel};

/// What tier of detail a deposit is currently shown at.
///
/// Mirrors the four "shapes" the issue body calls out (Unknown /
/// Class / Range / Precise). Used by the dossier and dashboard
/// pickers to choose the right row format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DepositVisibility {
    /// No information. The deposit is hidden from the dossier.
    Unknown,
    /// The class is known but no quantity. Shown as "Iron (?)" or
    /// the tile with no number.
    ClassOnly,
    /// The class and an estimate range. Shown as "Iron 100–200 Mt".
    Range,
    /// The class and the precise quantity. Shown as "Iron 175 Mt".
    Precise,
}

impl DepositVisibility {
    /// Short label for compact UI rows.
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Unknown => "?",
            Self::ClassOnly => "Class",
            Self::Range => "Range",
            Self::Precise => "Exact",
        }
    }
}

/// The full visibility estimate for a single deposit at a given
/// dimension fidelity.
///
/// `low` and `high` are megatons; `None` means that bound is
/// suppressed (used by `ClassOnly` and `Precise`). `mid` is the
/// best-estimate megaton count; `None` means the class is known but
/// the quantity is not.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DepositEstimate {
    pub visibility: DepositVisibility,
    /// Lower bound (megatons). `None` for `ClassOnly` / `Unknown`.
    pub low: Option<f64>,
    /// Best estimate (megatons). `None` for `Unknown`.
    pub mid: Option<f64>,
    /// Upper bound (megatons). `None` for `ClassOnly` / `Unknown`.
    pub high: Option<f64>,
    /// The dimension tier the estimate was derived from. Useful for
    /// tooltip text and for sorting.
    pub tier: u8,
    /// Confidence of the estimate, 0.0–1.0. The UI may show a
    /// "stale" icon when this is below `WARNING_CONFIDENCE`.
    pub confidence: f32,
}

impl DepositEstimate {
    /// A truly unsurveyed estimate (tier 0, confidence 0.0).
    pub const UNKNOWN: Self = Self {
        visibility: DepositVisibility::Unknown,
        low: None,
        mid: None,
        high: None,
        tier: 0,
        confidence: 0.0,
    };

    /// True if the estimate is precise enough to be summed into a
    /// mining total.
    pub fn is_quantified(&self) -> bool {
        matches!(
            self.visibility,
            DepositVisibility::Range | DepositVisibility::Precise
        ) && self.mid.is_some()
    }

    /// The best-estimate megatons, or 0.0 when the deposit is not
    /// yet quantified. Lets call sites `sum() / total()` without
    /// unwrapping.
    pub fn mid_or_zero(&self) -> f64 {
        self.mid.unwrap_or(0.0)
    }
}

/// Map a `MineralDeposits` dimension tier to the visibility tier
/// shown in the UI. The table is the canonical reference; tests in
/// `super::components` and at the bottom of this file lock it in.
fn visibility_for_tier(tier: u8) -> DepositVisibility {
    match tier {
        0 => DepositVisibility::Unknown,
        1 => DepositVisibility::ClassOnly,
        2..=4 => DepositVisibility::Range,
        5..=u8::MAX => DepositVisibility::Precise,
    }
}

/// Choose the reserve slice the player is allowed to see at a given
/// tier. Higher tiers reveal more of the deposit's
/// (proven, deep, bulk) split.
fn reserve_slice(deposit: &MineralDeposit, tier: u8) -> Option<ResourceReserve> {
    let r = &deposit.reserve;
    match tier {
        0 => None,
        1 => None,
        2 => Some(ResourceReserve::new(
            r.proven_crustal,
            0.0,
            0.0,
            r.concentration,
        )),
        3 => Some(ResourceReserve::new(
            r.proven_crustal,
            r.deep_deposits,
            0.0,
            r.concentration,
        )),
        4..=u8::MAX => Some(ResourceReserve::new(
            r.proven_crustal,
            r.deep_deposits,
            r.planetary_bulk,
            r.concentration,
        )),
    }
}

/// Compute the deposit estimate for a `MineralDeposits` dimension
/// fidelity. The legacy `discovered_amount(SurveyLevel)` call sites
/// in the dossier, dashboard, and economy panel route through this
/// function via `SurveyLevel::as_deposit_fidelity`.
pub fn estimate_with_fidelity(
    deposit: &MineralDeposit,
    fidelity: DimensionFidelity,
) -> DepositEstimate {
    let tier = fidelity.tier.min(MAX_TIER);
    let visibility = visibility_for_tier(tier);
    let slice = reserve_slice(deposit, tier);
    match visibility {
        DepositVisibility::Unknown => DepositEstimate::UNKNOWN,
        DepositVisibility::ClassOnly => DepositEstimate {
            visibility,
            low: None,
            mid: None,
            high: None,
            tier,
            confidence: fidelity.confidence,
        },
        DepositVisibility::Range | DepositVisibility::Precise => {
            let Some(slice) = slice else {
                return DepositEstimate::UNKNOWN;
            };
            let mid = slice.total_mass();
            // Band factor widens as confidence drops. At confidence
            // 1.0 the band is the tier base × 0.5; at confidence
            // 0.0 the band is the tier base × 1.0.
            let base_band = match visibility {
                DepositVisibility::Precise => 0.10_f32,
                DepositVisibility::Range => 0.45,
                _ => 0.45,
            };
            let confidence_factor = 1.0 - (fidelity.confidence * 0.5).clamp(0.0, 0.5);
            let band = (base_band * confidence_factor) as f64;
            let low = (mid * (1.0 - band)).max(0.0);
            let high = mid * (1.0 + band);
            DepositEstimate {
                visibility,
                low: Some(low),
                mid: Some(mid),
                high: Some(high),
                tier,
                confidence: fidelity.confidence,
            }
        }
    }
}

impl SurveyLevel {
    /// 1:1 adapter for the migration shim. Maps the legacy enum to
    /// a `MineralDeposits` dimension fidelity so that
    /// [`estimate_with_fidelity`] can produce an estimate for bodies
    /// that only have the legacy component.
    ///
    /// The mapping follows SURVEY_REWORK.md §15:
    ///
    /// | `SurveyLevel`      | MineralDeposits tier | Confidence |
    /// |--------------------|----------------------|------------|
    /// | `Unsurveyed`       | 0                    | 0.0        |
    /// | `OrbitalScan`      | 1                    | 0.5        |
    /// | `SeismicSurvey`    | 2                    | 0.7        |
    /// | `CoreSample`       | 5                    | 0.95       |
    pub fn as_deposit_fidelity(self, sim_time: f64) -> DimensionFidelity {
        match self {
            SurveyLevel::Unsurveyed => DimensionFidelity::UNKNOWN,
            SurveyLevel::OrbitalScan => DimensionFidelity::at_tier(1, 0.5, Some(sim_time)),
            SurveyLevel::SeismicSurvey => DimensionFidelity::at_tier(2, 0.7, Some(sim_time)),
            SurveyLevel::CoreSample => DimensionFidelity::at_tier(MAX_TIER, 0.95, Some(sim_time)),
        }
    }
}

/// True if the fidelity's confidence has dropped below the warning
/// threshold (the data is stale). UI surfaces this as a warning
/// icon.
pub fn is_stale(fidelity: DimensionFidelity) -> bool {
    fidelity.confidence < WARNING_CONFIDENCE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::types::ResourcePhase;

    fn iron() -> MineralDeposit {
        MineralDeposit {
            reserve: ResourceReserve::new(1_000.0, 5_000.0, 50_000.0, 0.8),
            accessibility: 0.6,
            is_atmospheric: false,
            phase: ResourcePhase::Solid,
        }
    }

    fn estimate(tier: u8, confidence: f32) -> DepositEstimate {
        estimate_with_fidelity(
            &iron(),
            DimensionFidelity::at_tier(tier, confidence, Some(0.0)),
        )
    }

    #[test]
    fn unknown_hides_quantity() {
        let e = estimate(0, 0.0);
        assert_eq!(e.visibility, DepositVisibility::Unknown);
        assert_eq!(e.mid_or_zero(), 0.0);
        assert!(!e.is_quantified());
    }

    #[test]
    fn class_only_at_tier_one() {
        let e = estimate(1, 0.5);
        assert_eq!(e.visibility, DepositVisibility::ClassOnly);
        assert!(e.mid.is_none());
        assert!(!e.is_quantified());
    }

    #[test]
    fn range_at_tier_two_uses_proven_only() {
        let e = estimate(2, 0.7);
        assert_eq!(e.visibility, DepositVisibility::Range);
        assert_eq!(e.mid, Some(1_000.0));
        // Confidence 0.7 → band factor 1.0 - 0.35 = 0.65, base 0.45
        // → band 0.2925. Low/high around 1_000 Mt.
        let (low, high) = (e.low.unwrap(), e.high.unwrap());
        assert!(low < 1_000.0);
        assert!(high > 1_000.0);
        assert!(low > 0.0);
    }

    #[test]
    fn range_at_tier_three_includes_deep() {
        let e = estimate(3, 0.7);
        assert_eq!(e.visibility, DepositVisibility::Range);
        // proven_crustal + deep_deposits = 1_000 + 5_000
        assert_eq!(e.mid, Some(6_000.0));
    }

    #[test]
    fn precise_at_tier_five_uses_full_reserve() {
        let e = estimate(5, 0.95);
        assert_eq!(e.visibility, DepositVisibility::Precise);
        // Full reserve: 1_000 + 5_000 + 50_000
        assert_eq!(e.mid, Some(56_000.0));
        // Narrow band at high confidence.
        let band_pct = (e.high.unwrap() - e.mid.unwrap()) / e.mid.unwrap();
        assert!(band_pct < 0.15);
    }

    #[test]
    fn band_widens_at_low_confidence() {
        let high = estimate(3, 0.95);
        let low = estimate(3, 0.1);
        let high_band = high.high.unwrap() - high.mid.unwrap();
        let low_band = low.high.unwrap() - low.mid.unwrap();
        assert!(
            low_band > high_band,
            "low confidence {low_band} should produce a wider band than high confidence {high_band}"
        );
    }

    #[test]
    fn tier_clamps_to_max() {
        let e = estimate(255, 0.9);
        assert_eq!(e.tier, MAX_TIER);
        assert_eq!(e.visibility, DepositVisibility::Precise);
    }

    #[test]
    fn legacy_survey_level_mapping() {
        let sim = 42.0;
        let f = SurveyLevel::Unsurveyed.as_deposit_fidelity(sim);
        assert_eq!(f.tier, 0);
        let f = SurveyLevel::OrbitalScan.as_deposit_fidelity(sim);
        assert_eq!(f.tier, 1);
        let f = SurveyLevel::SeismicSurvey.as_deposit_fidelity(sim);
        assert_eq!(f.tier, 2);
        let f = SurveyLevel::CoreSample.as_deposit_fidelity(sim);
        assert_eq!(f.tier, MAX_TIER);
        assert!(f.confidence > 0.9);
    }
}
