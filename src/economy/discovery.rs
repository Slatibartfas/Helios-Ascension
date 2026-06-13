//! GRA-111 — 3-tier resource reveal matrix helpers.
//!
//! Bridges the planet dossier's "REVEAL MATRIX" section to the v0.5.0
//! per-body [`SurveyState`](crate::survey::components::SurveyState).
//! The matrix is a per-resource, 3-row sub-table showing the discovery
//! state of the body's `ResourceReserve` per depth tier
//! (`Proven Crustal` / `Deep Deposits` / `Planetary Bulk`).
//!
//! Design contract — see [GRA-110](https://paperclip.klingspor.one/GRA/issues/GRA-110)
//! and the LGD hand-off in the GRA-109 comments. Schema changes here
//! are additive; the existing call sites for the legacy single-scalar
//! `discovered_amount` are untouched.
//!
//! ## Gate summary
//!
//! | Tier | Survey dimension gate | Additional gate |
//! |------|----------------------|------------------|
//! | 1 Proven Crustal | `MineralDeposits.tier >= 2` | `Reserve.proven_crustal > 0.001` |
//! | 2 Deep Deposits   | `SubsurfaceStructure.tier >= 3` | `Reserve.deep_deposits > 0.001` |
//! | 3 Planetary Bulk  | `SubsurfaceStructure.tier >= 5` | `drill_missions_completed >= 1` |
//!
//! Atmospheric deposits (`MineralDeposit::is_atmospheric == true`)
//! and `Option::None` survey states collapse to a `revealed: false`
//! triple with `concentration: None`. Reserve fields that fall
//! below the 0.001 Mt threshold used in `mining.rs:324,335,351` map
//! to `revealed: false` (treating the deposit as not present at
//! that depth, regardless of survey progress).

use crate::economy::components::{MineralDeposit, ResourceReserve, SurveyLevel};
use crate::economy::types::ResourceType;
use crate::survey::components::SurveyState;
use crate::survey::types::SurveyDimension;

/// Per-tier label used by the dossier's `draw_reveal_matrix` render
/// function. Modders can add new tier labels by extending this enum,
/// but the 3-row display is locked to 3 entries for v0.5.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierLabel {
    /// T1: shallow surface deposits. Label changes for atmospheric
    /// deposits (`Atmospheric`).
    ProvenCrustal,
    /// T2: km-deep ore bodies. Label changes for atmospheric
    /// deposits (`Trapped / Dissolved`).
    DeepDeposits,
    /// T3: mantle / core concentrations. Label changes for
    /// atmospheric deposits (`Chemically Bound`).
    PlanetaryBulk,
}

impl TierLabel {
    /// Index into the `[TierReveal; 3]` returned by
    /// `MineralDeposit::tier_breakdown`. Matches the per-resource
    /// visual order in the dossier.
    pub const ALL: [TierLabel; 3] = [
        TierLabel::ProvenCrustal,
        TierLabel::DeepDeposits,
        TierLabel::PlanetaryBulk,
    ];

    /// Display label for the dossier. The mineral names come from
    /// `SURVEY_REWORK.md` L382–390; the atmospheric variants are
    /// from `economy/components.rs:209-211`.
    pub fn display(&self, is_atmospheric: bool) -> &'static str {
        match (self, is_atmospheric) {
            (TierLabel::ProvenCrustal, false) => "Proven Crustal",
            (TierLabel::ProvenCrustal, true) => "Atmospheric",
            (TierLabel::DeepDeposits, false) => "Deep Deposits",
            (TierLabel::DeepDeposits, true) => "Trapped / Dissolved",
            (TierLabel::PlanetaryBulk, false) => "Planetary Bulk",
            (TierLabel::PlanetaryBulk, true) => "Chemically Bound",
        }
    }

    /// Human-readable threshold text shown in dimmed rows. The
    /// player reads this to know what to survey next.
    pub fn threshold_text(&self) -> &'static str {
        match self {
            TierLabel::ProvenCrustal => "MineralDeposits \u{2265} 2",
            TierLabel::DeepDeposits => "Subsurface \u{2265} 3 + Mineral \u{2265} 2",
            TierLabel::PlanetaryBulk => "Subsurface \u{2265} 5 + drill",
        }
    }

    /// Single-character mono chip used as a tier badge. The LGD
    /// preferred the explicit `T1`/`T2`/`T3` over a `▮▯` fill chip
    /// for screen-reader / modder clarity.
    pub fn tier_badge(&self) -> &'static str {
        match self {
            TierLabel::ProvenCrustal => "T1",
            TierLabel::DeepDeposits => "T2",
            TierLabel::PlanetaryBulk => "T3",
        }
    }
}

/// Per-tier display state for the dossier's 3-row reveal matrix.
/// The dossier renders one `TierReveal` per
/// [`TierLabel`] in the order `Proven Crustal → Deep Deposits →
/// Planetary Bulk`.
///
/// `gate` is the coarse-grained `SurveyLevel` (OrbitalScan /
/// SeismicSurvey / CoreSample) that the tier corresponds to —
/// surfaced as a chip in the row header. It is the same coarse
/// grain as the dossier's existing `SurveyLevel` badge, not the
/// raw per-dimension tier.
///
/// `revealed: false` collapses the row to a dimmed placeholder.
/// `megatons: None` and `concentration: None` mean the body has
/// no deposit at this depth; the matrix still renders the row
/// (showing the threshold text) so the player can see what
/// surveying will unlock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TierReveal {
    /// Coarse gate level (OrbitalScan / SeismicSurvey / CoreSample).
    /// Surfaces in the row header as a chip.
    pub gate: super::components::SurveyLevel,
    /// Per-tier mass in megatons. `None` if `revealed == false` or
    /// the body has no deposit at this depth. `Some(0.0)` is fine —
    /// it's distinct from `None` and means "revealed but depleted".
    pub megatons: Option<f64>,
    /// Per-tier concentration (mineral deposits only). `None` for
    /// atmospheric deposits and for unrevealed rows.
    pub concentration: Option<f32>,
    /// Whether the body's `SurveyState` has reached this tier. When
    /// `false`, the dossier dims the row and shows the threshold
    /// text from [`TierLabel::threshold_text`].
    pub revealed: bool,
}

impl Default for TierReveal {
    fn default() -> Self {
        Self {
            gate: super::components::SurveyLevel::Unsurveyed,
            megatons: None,
            concentration: None,
            revealed: false,
        }
    }
}

impl TierReveal {
    /// Coarse `SurveyLevel` badge for a given tier. Matches the
    /// three legacy enum variants — the dossier's existing status
    /// badge uses the same palette (see
    /// `dossier_panel.rs:1513-1518`).
    pub fn gate_for(label: TierLabel) -> super::components::SurveyLevel {
        match label {
            TierLabel::ProvenCrustal => SurveyLevel::OrbitalScan,
            TierLabel::DeepDeposits => SurveyLevel::SeismicSurvey,
            TierLabel::PlanetaryBulk => SurveyLevel::CoreSample,
        }
    }
}

/// Threshold (in megatons) below which a `ResourceReserve` field
/// is treated as "not present" at that depth. Matches the existing
/// `> 0.001` checks in `src/economy/mining.rs:324,335,351`. Mirrored
/// here so a future PR that rebalances the threshold only has to
/// touch one constant.
pub const RESERVE_PRESENT_THRESHOLD: f64 = 0.001;

impl MineralDeposit {
    /// Per-tier breakdown for the dossier's reveal matrix.
    /// Returns `[T1, T2, T3]` in [`TierLabel::ALL`] order.
    ///
    /// Behaviour:
    /// - `is_atmospheric == true` → all three `revealed: false`,
    ///   `concentration: None` (atmospheric deposits don't carry
    ///   the existing per-tier mass from the same `ResourceReserve`
    ///   layout — the dossier just shows the threshold text).
    /// - `state == None` → all three `revealed: false` (no panic,
    ///   matching the GRA-110 §[Atmospheric pre-filter] call).
    /// - `Reserve.<field> <= RESERVE_PRESENT_THRESHOLD` →
    ///   `revealed: false` at that tier (deposit doesn't exist at
    ///   this depth on this body), regardless of survey progress.
    /// - T3 (Planetary Bulk) also requires
    ///   `state.drill_missions_completed >= 1` (see
    ///   [`SurveyState::planetary_bulk_unlocked`]).
    pub fn tier_breakdown(&self, state: Option<&SurveyState>) -> [TierReveal; 3] {
        tier_breakdown_for_reserve(self.reserve, self.is_atmospheric, state)
    }

    /// Convenience: label and display info for a tier, given the
    /// atmospheric flag. Reduces 3-line match expressions in the
    /// dossier's render fn to a single helper call.
    pub fn tier_label(&self, label: TierLabel) -> &'static str {
        label.display(self.is_atmospheric)
    }
}

/// Pure helper: produce a 3-tier breakdown for a [`ResourceReserve`]
/// given the atmospheric flag and (optional) survey state.
///
/// Pulled out of [`MineralDeposit::tier_breakdown`] so it can be
/// unit-tested without an ECS world. The function takes a copy of
/// the reserve (it's a small `Copy` struct) and a borrowed
/// `SurveyState`; no `&mut` is needed.
pub fn tier_breakdown_for_reserve(
    reserve: ResourceReserve,
    is_atmospheric: bool,
    state: Option<&SurveyState>,
) -> [TierReveal; 3] {
    // Atmospheric deposits don't carry the per-tier mineral
    // `ResourceReserve` layout — the dossier collapses the 3 rows
    // to dimmed placeholders with the threshold text visible.
    if is_atmospheric {
        return [
            TierReveal {
                gate: TierReveal::gate_for(TierLabel::ProvenCrustal),
                megatons: None,
                concentration: None,
                revealed: false,
            },
            TierReveal {
                gate: TierReveal::gate_for(TierLabel::DeepDeposits),
                megatons: None,
                concentration: None,
                revealed: false,
            },
            TierReveal {
                gate: TierReveal::gate_for(TierLabel::PlanetaryBulk),
                megatons: None,
                concentration: None,
                revealed: false,
            },
        ];
    }

    // No SurveyState (Phase 1 migration window, or a brand-new body
    // whose SurveyState hasn't been inserted yet). All three rows
    // collapse to dimmed placeholders. The render fn still surfaces
    // the threshold text so the player sees what to survey.
    let Some(state) = state else {
        return [
            TierReveal {
                gate: TierReveal::gate_for(TierLabel::ProvenCrustal),
                megatons: None,
                concentration: None,
                revealed: false,
            },
            TierReveal {
                gate: TierReveal::gate_for(TierLabel::DeepDeposits),
                megatons: None,
                concentration: None,
                revealed: false,
            },
            TierReveal {
                gate: TierReveal::gate_for(TierLabel::PlanetaryBulk),
                megatons: None,
                concentration: None,
                revealed: false,
            },
        ];
    };

    // Snapshot the dimension tiers once. `DimensionFidelity::tier`
    // is `u8` so the copies are cheap.
    let mineral_tier = state.fidelity(SurveyDimension::MineralDeposits).tier;
    let subsurface_tier = state.fidelity(SurveyDimension::Subsurface).tier;

    let t1_present = reserve.proven_crustal > RESERVE_PRESENT_THRESHOLD;
    let t2_present = reserve.deep_deposits > RESERVE_PRESENT_THRESHOLD;
    let t3_present = reserve.planetary_bulk > RESERVE_PRESENT_THRESHOLD;

    // Gate logic per GRA-110 §[Q1 / Q2 / Q3] and the SURVEY_REWORK
    // design doc:
    //   T1 = MineralDeposits.tier >= 2 (no drill requirement)
    //   T2 = Subsurface.tier >= 3 AND MineralDeposits.tier >= 2
    //   T3 = Subsurface.tier >= 5 AND drill_missions_completed >= 1
    let t1_unlocked = mineral_tier >= 2 && t1_present;
    let t2_unlocked = subsurface_tier >= 3 && mineral_tier >= 2 && t2_present;
    let t3_unlocked = state.planetary_bulk_unlocked() && t3_present;

    [
        TierReveal {
            gate: TierReveal::gate_for(TierLabel::ProvenCrustal),
            megatons: if t1_unlocked {
                Some(reserve.proven_crustal)
            } else {
                None
            },
            concentration: if t1_unlocked {
                Some(reserve.concentration)
            } else {
                None
            },
            revealed: t1_unlocked,
        },
        TierReveal {
            gate: TierReveal::gate_for(TierLabel::DeepDeposits),
            megatons: if t2_unlocked {
                Some(reserve.deep_deposits)
            } else {
                None
            },
            concentration: if t2_unlocked {
                Some(reserve.concentration)
            } else {
                None
            },
            revealed: t2_unlocked,
        },
        TierReveal {
            gate: TierReveal::gate_for(TierLabel::PlanetaryBulk),
            megatons: if t3_unlocked {
                Some(reserve.planetary_bulk)
            } else {
                None
            },
            // T3 is deep / planetary-scale ore; the per-reserve
            // `concentration` still describes the mineral phase
            // (it's a single float per deposit, not per tier) so we
            // surface it whenever the row is revealed. The dossier
            // doc-strip stays consistent with T1 / T2.
            concentration: if t3_unlocked {
                Some(reserve.concentration)
            } else {
                None
            },
            revealed: t3_unlocked,
        },
    ]
}

/// Aggregate the dossier's per-resource tier breakdowns into a
/// body-level 3-row header. Each `TierReveal` field on the result
/// is `revealed: true` iff at least one contributing deposit has
/// the tier `revealed`. Megatons are summed across revealed rows.
/// Used by `draw_reveal_matrix` to surface the body-aggregate
/// header row described in the LGD design contract.
pub fn body_aggregate_tier_breakdown(
    resources: &crate::economy::components::PlanetResources,
    state: Option<&SurveyState>,
) -> [TierReveal; 3] {
    let mut counts = [0u32; 3];
    let mut total_megatons = [0.0_f64; 3];
    let mut max_concentration = [None; 3];
    let mut any_revealed = [false; 3];

    for (_resource, deposit) in resources.deposits.iter() {
        let breakdown = deposit.tier_breakdown(state);
        for (i, reveal) in breakdown.iter().enumerate() {
            if reveal.revealed {
                any_revealed[i] = true;
                counts[i] += 1;
                if let Some(mt) = reveal.megatons {
                    total_megatons[i] += mt;
                }
                if let Some(conc) = reveal.concentration {
                    max_concentration[i] = Some(
                        max_concentration[i]
                            .map(|c: f32| c.max(conc))
                            .unwrap_or(conc),
                    );
                }
            }
        }
    }

    [
        TierReveal {
            gate: TierReveal::gate_for(TierLabel::ProvenCrustal),
            megatons: if any_revealed[0] {
                Some(total_megatons[0])
            } else {
                None
            },
            concentration: max_concentration[0],
            revealed: any_revealed[0],
        },
        TierReveal {
            gate: TierReveal::gate_for(TierLabel::DeepDeposits),
            megatons: if any_revealed[1] {
                Some(total_megatons[1])
            } else {
                None
            },
            concentration: max_concentration[1],
            revealed: any_revealed[1],
        },
        TierReveal {
            gate: TierReveal::gate_for(TierLabel::PlanetaryBulk),
            megatons: if any_revealed[2] {
                Some(total_megatons[2])
            } else {
                None
            },
            concentration: max_concentration[2],
            revealed: any_revealed[2],
        },
    ]
    .map(|reveal| {
        // Drop the resource-count bookkeeping — the dossier's
        // render fn reads `revealed` / `megatons` / `concentration`
        // and surfaces the per-body counts separately.
        let _ = counts;
        reveal
    })
}

/// Resource-type helper: returns `true` for `ResourceType`s that
/// the dossier's reveal matrix should collapse to a single dimmed
/// "v0.5.x follow-up" row instead of the standard 3-row layout.
///
/// Per GRA-110 §[Q4], the LGD agreed the v0.5.0 build should not
/// pattern-match on resource names ("He-3" specifically). Instead,
/// the data layer carries a `follow_up_only: bool` flag. The
/// placeholder implementation here is a static allowlist — the
/// RON-driven flag in [`MineralDeposit`] will replace it once
/// the v0.5.0 RON files have it in (planned for v0.5.x, not v0.5.0).
pub fn is_follow_up_only_resource(resource: ResourceType) -> bool {
    matches!(
        resource,
        ResourceType::Helium3 | ResourceType::Antimatter | ResourceType::Metamaterials
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::components::{MineralDeposit, ResourceReserve};
    use crate::economy::types::ResourcePhase;
    use crate::survey::components::DimensionFidelity;

    fn make_state(mineral_tier: u8, subsurface_tier: u8, drill_done: u32) -> SurveyState {
        let mut s = SurveyState::default();
        s.set_fidelity(
            SurveyDimension::MineralDeposits,
            DimensionFidelity::at_tier(mineral_tier, 1.0, Some(0.0)),
        );
        s.set_fidelity(
            SurveyDimension::Subsurface,
            DimensionFidelity::at_tier(subsurface_tier, 1.0, Some(0.0)),
        );
        s.drill_missions_completed = drill_done;
        s
    }

    fn mineral_reserve(proven: f64, deep: f64, bulk: f64) -> ResourceReserve {
        ResourceReserve::new(proven, deep, bulk, 0.5)
    }

    fn mineral_deposit(reserve: ResourceReserve) -> MineralDeposit {
        MineralDeposit {
            reserve,
            accessibility: 0.5,
            is_atmospheric: false,
            phase: ResourcePhase::Solid,
        }
    }

    fn atmospheric_deposit() -> MineralDeposit {
        MineralDeposit {
            reserve: mineral_reserve(0.0, 0.0, 0.0),
            accessibility: 0.0,
            is_atmospheric: true,
            phase: ResourcePhase::Vapor,
        }
    }

    #[test]
    fn no_survey_state_collapses_to_three_dimmed_rows() {
        let reserve = mineral_reserve(100.0, 200.0, 300.0);
        let deposit = mineral_deposit(reserve);
        let breakdown = deposit.tier_breakdown(None);
        assert_eq!(breakdown.len(), 3);
        for reveal in &breakdown {
            assert!(!reveal.revealed);
            assert!(reveal.megatons.is_none());
            assert!(reveal.concentration.is_none());
        }
    }

    #[test]
    fn atmospheric_deposit_collapses_to_three_dimmed_rows() {
        let deposit = atmospheric_deposit();
        let state = make_state(5, 5, 1);
        let breakdown = deposit.tier_breakdown(Some(&state));
        for reveal in &breakdown {
            assert!(!reveal.revealed);
            assert!(reveal.megatons.is_none());
            assert!(reveal.concentration.is_none());
        }
    }

    #[test]
    fn zero_reserve_collapses_to_three_dimmed_rows() {
        // 0.001 Mt is the existing threshold in mining.rs — values
        // at or below that map to "not present" at that depth.
        let reserve = mineral_reserve(0.0005, 0.0005, 0.0005);
        let deposit = mineral_deposit(reserve);
        let state = make_state(5, 5, 1);
        let breakdown = deposit.tier_breakdown(Some(&state));
        for reveal in &breakdown {
            assert!(!reveal.revealed, "{reveal:?} should be unrevealed");
        }
    }

    #[test]
    fn proven_crustal_reveals_at_mineral_tier_2() {
        let reserve = mineral_reserve(100.0, 200.0, 300.0);
        let deposit = mineral_deposit(reserve);
        // MineralDeposits tier 1 → not enough for T1
        let state = make_state(1, 5, 1);
        let breakdown = deposit.tier_breakdown(Some(&state));
        assert!(!breakdown[0].revealed);
        assert!(!breakdown[1].revealed);
        assert!(
            breakdown[2].revealed,
            "T3 should still open at subsurface 5 + drill"
        );
        // MineralDeposits tier 2 → T1 opens
        let state = make_state(2, 0, 0);
        let breakdown = deposit.tier_breakdown(Some(&state));
        assert!(breakdown[0].revealed);
        assert!(!breakdown[1].revealed);
        assert!(!breakdown[2].revealed);
    }

    #[test]
    fn deep_deposits_requires_subsurface_t3_and_mineral_t2() {
        let reserve = mineral_reserve(100.0, 200.0, 300.0);
        let deposit = mineral_deposit(reserve);
        // T2 needs both subsurface >= 3 AND mineral >= 2
        let state = make_state(2, 2, 0);
        let breakdown = deposit.tier_breakdown(Some(&state));
        assert!(breakdown[0].revealed);
        assert!(!breakdown[1].revealed);
        let state = make_state(2, 3, 0);
        let breakdown = deposit.tier_breakdown(Some(&state));
        assert!(breakdown[1].revealed);
        assert!(breakdown[0].revealed);
        assert!(!breakdown[2].revealed);
    }

    #[test]
    fn planetary_bulk_requires_subsurface_t5_and_drill() {
        let reserve = mineral_reserve(100.0, 200.0, 300.0);
        let deposit = mineral_deposit(reserve);
        // Subsurface 5 + drill 1 → T3 open
        let state = make_state(2, 5, 1);
        let breakdown = deposit.tier_breakdown(Some(&state));
        assert!(breakdown[2].revealed);
        // Subsurface 5 but no drill → T3 locked
        let state = make_state(2, 5, 0);
        let breakdown = deposit.tier_breakdown(Some(&state));
        assert!(!breakdown[2].revealed);
        // Subsurface 4 + drill 1 → T3 still locked
        let state = make_state(2, 4, 1);
        let breakdown = deposit.tier_breakdown(Some(&state));
        assert!(!breakdown[2].revealed);
    }

    #[test]
    fn reveal_megatons_match_reserve_field_when_revealed() {
        let reserve = mineral_reserve(100.0, 200.0, 300.0);
        let deposit = mineral_deposit(reserve);
        let state = make_state(2, 5, 1);
        let breakdown = deposit.tier_breakdown(Some(&state));
        assert_eq!(breakdown[0].megatons, Some(100.0));
        assert_eq!(breakdown[1].megatons, Some(200.0));
        assert_eq!(breakdown[2].megatons, Some(300.0));
    }

    #[test]
    fn tier_labels_match_atmospheric_flag() {
        let state = make_state(5, 5, 1);
        let mineral = mineral_deposit(mineral_reserve(1.0, 1.0, 1.0));
        let atmo = atmospheric_deposit();
        assert_eq!(
            mineral.tier_label(TierLabel::ProvenCrustal),
            "Proven Crustal"
        );
        assert_eq!(atmo.tier_label(TierLabel::ProvenCrustal), "Atmospheric");
        assert_eq!(mineral.tier_label(TierLabel::DeepDeposits), "Deep Deposits");
        assert_eq!(
            atmo.tier_label(TierLabel::DeepDeposits),
            "Trapped / Dissolved"
        );
        assert_eq!(
            mineral.tier_label(TierLabel::PlanetaryBulk),
            "Planetary Bulk"
        );
        assert_eq!(
            atmo.tier_label(TierLabel::PlanetaryBulk),
            "Chemically Bound"
        );
        // Unused state binding to silence the warning about the
        // `state` parameter not being needed for this test.
        let _ = state;
    }

    #[test]
    fn body_aggregate_sums_revealed_tier_megatons() {
        use crate::economy::components::PlanetResources;
        use std::collections::HashMap;

        let mut resources = PlanetResources::default();
        let mut deposits = HashMap::new();
        deposits.insert(
            ResourceType::Iron,
            mineral_deposit(mineral_reserve(100.0, 200.0, 300.0)),
        );
        deposits.insert(
            ResourceType::Silicates,
            mineral_deposit(mineral_reserve(50.0, 80.0, 120.0)),
        );
        resources.deposits = deposits;
        let state = make_state(2, 5, 1);
        let aggregate = body_aggregate_tier_breakdown(&resources, Some(&state));
        assert!(aggregate[0].revealed);
        assert!(aggregate[1].revealed);
        assert!(aggregate[2].revealed);
        assert_eq!(aggregate[0].megatons, Some(150.0));
        assert_eq!(aggregate[1].megatons, Some(280.0));
        assert_eq!(aggregate[2].megatons, Some(420.0));
    }
}
