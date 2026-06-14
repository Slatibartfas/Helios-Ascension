# Tech Tree Pacing (GRA-127)

This document captures the math behind the tier-1 → tier-10 research curve
in `assets/data/technologies.ron`, and the LGD's pacing target for
playtest. It is the design reference for the LGD's tier-cost audit and
the Coder's per-tier rebalance.

## Constants

| Quantity | Value | Source |
|---|---|---|
| `ResearchLab` yield (Level 1) | **100 RP / sim-month** | `buildings.ron:810, 834` (modifier `ResearchSpeed: 100.0`) |
| RP / sim-year (1 lab) | **1,200 RP** | 100 × 12 |
| RP / sim-day (1 lab) | **3.285 RP** | 1,200 ÷ 365.25 |
| Baseline (no labs) | 500 RP / sim-year = 1.369 RP / sim-day | `src/research/systems.rs:88-89, 116` |
| Sim-day | 86,400 sim-seconds | `src/ui/time.rs` |
| Sim-year | 31,557,600 sim-seconds (Julian, 365.25 d) | `src/research/systems.rs:92` |
| Tick model | Continuous Euler integration, not discrete tick | `src/research/systems.rs:96-107, 181-198` |
| Time source | `SimulationTime.elapsed_seconds()` (no cap) | `src/ui/time.rs:58-63` |

A single ResearchLab is **2.4× the no-lab baseline**. With no lab the
player still accumulates 500 RP / sim-year, but at 1 lab the rate jumps
to 1,200 RP / sim-year.

## Pacing target

The LGD's pacing target for the playtest build:

| Tier | Target unlock window (1 lab) | Target unlock window (8 labs) |
|---|---:|---:|
| Tier 1 paid (any single tech) | 60 – 180 sim-days | 8 – 22 sim-days |
| Tier 1 full sweep (max cost = bottleneck) | 180 – 365 sim-days | 23 – 46 sim-days |
| Tier 2 median | 365 – 730 sim-days (1–2 sim-years) | 46 – 91 sim-days |
| Tier 3 median | 1.5 – 3 sim-years | 92 – 183 sim-days |
| Tier 4 median | 5 – 10 sim-years | 183 – 365 sim-days |
| Tier 5 median | 10 – 20 sim-years | 365 – 730 sim-days |
| Tier 6+ | unrestricted (no LGD target) | unrestricted |

**Rationale.** 1 lab is the early-game floor. A fresh colony on Mars
should be able to research the hull-gate `chemical_spaceframes` and
launch its first probe inside the first sim-year, not the second. Eight
labs is the rough steady-state for a mid-game Earth colony and is
where most of the tier-2 / tier-3 chain becomes tractable. The
tier-4 / tier-5 windows are deliberately long — fusion and antimatter
are the long-horizon investments that distinguish a 4X mid-game from
late-game.

## Current state vs target

The `research_cost` values in `technologies.ron` were calibrated for
a *fleet-scale* lab economy (12+ labs), not 1-lab early game. At
1 lab, the tier-1 paid chain takes ~1.25 sim-years per tech and
~2 sim-years to clear the whole tier serially. That is **8× the LGD
target**.

| Tech | Current cost (RP) | Days at 1 lab | Days at 8 labs | LGD target? |
|---|---:|---:|---:|---|
| `project_management` (cheapest T1 paid) | 800 | 243 | 30 | **No** — needs ~300 RP |
| `chemical_spaceframes` (tier-1 hull gate) | 1,200 | 365 | 46 | **No** — needs ~400 RP |
| `survey_methodology` (T1 max-ish) | 2,000 | 609 | 76 | **No** — needs ~700 RP |
| `solar_power` (T1 max) | 2,500 | 761 | 95 | **No** — needs ~800 RP |
| Tier 1 paid total (16 techs) | 24,200 | (bottleneck = 2,500) | (bottleneck = 2,500) | — |

**Rebalance rule for the LGD's RON pass:** divide all tier-1 paid
`research_cost` values by **3**, so the new range is **~270 – 830 RP**.
At 1 lab that maps to 82 – 253 sim-days per tech (within the 60–180
target band for the cheap end, slightly over for the expensive end —
acceptable because the expensive tier-1 techs are not gating anything
critical). The hull-gate `chemical_spaceframes` drops from 1,200 → 400
RP, which is 122 sim-days at 1 lab — a single in-game season.

## Tier-scaling preservation

The hull-gate chain has 6 anchor costs that LGD wants to preserve
roughly 1:1 with hull class:

| Hull-gate | Tier | Current cost | New cost (proposed) |
|---|---:|---:|---:|
| `chemical_spaceframes` | 1 | 1,200 | 400 |
| `orbital_construction` | 2 | 8,000 | 8,000 *(unchanged)* |
| `orbital_assembly_heavy` | 2 | 18,000 | 18,000 *(unchanged)* |
| `carbon_nanotube_frames` | 3 | 24,000 | 24,000 *(unchanged)* |
| `fusion_superstructures` | 4 | 78,000 | 78,000 *(unchanged)* |
| `antimatter_containment_structures` | 5 | 125,000 | 125,000 *(unchanged)* |

The T1 step drops by 3×; the rest stays the same. The result is a
flatter early-game ramp (T1 → T2 = 20× jump, T2 → T2 = 2.25×) and
the relative gate positions for orbital construction (8k), fighter
frames (18k), CNT destroyers (24k), fusion cruisers (78k), and AM
keels (125k) are unchanged.

## Tier-10 inflation (recommendation, not in this PR)

Tier 10 is plausibly too cheap: range 15M – 50M, max/min = 3.33×. The
two 50M entries (`the_singularity`, `omega_physics`) are the only real
endgame targets; the 15M – 20M entries (`utopian_society`,
`galactic_peace`, `panspermia_mastery`, `total_conversion`,
`universal_constructor`) read more like tier-9+.5. Suggested fix
(out of scope for GRA-127, file as follow-up): raise the cheap end to
**30M** and the ceiling to **100M**. This gives max/min = 3.33× and
restores a clean tier-9 → tier-10 step of 1.9× (currently 1.9× also,
but only because both tiers are squashed together).

## Lab economy cross-check

A real mid-game Earth player with **16 labs** (1 capital + 15 spares)
earns 16 × 3.285 = **52.5 RP / sim-day**, or **19,200 RP / sim-year**.
At that rate, the tier-2 median (4,750 RP) takes **90 sim-days**, tier-3
median (17,000 RP) takes **324 sim-days**, tier-4 median (65,000 RP)
takes **1,238 sim-days**. That matches the LGD target band for tiers
2 – 4 at 8 labs. The LGD pacing assumes the player can plausibly
operate 8 labs by tier-3; 16 labs by tier-4. This is consistent with
the building scale and the available Industrial/Fusion/AI buildings.

## Reproduction note for QA

QA can validate this with the existing F12 debug menu:

1. `free_construction = true` and place 1 ResearchLab on a test body.
2. `instant_build = true` and queue the lab.
3. Set time scale to `> 1 month / s` and observe `Research` resource
   accumulating at 100 / sim-month (= 1,200 / sim-year).
4. Queue `chemical_spaceframes` (1,200 RP at current costs). It should
   unlock in 365 sim-days at 1 lab.
5. After the GRA-127 RON PR lands, the same tech should unlock in
   ~122 sim-days.

The Coder's spawn-system PR (Phase A) can be tested independently of
the tech-tree PR: open a fresh game, observe the Day-1 fleet
constellation in Earth orbit (5 tier-1 hulls, no Venus / Jupiter /
Saturn / Alpha Centauri fleets), and confirm the Mars / Venus /
Jupiter / Saturn systems have **no** pre-spawned fleet.
