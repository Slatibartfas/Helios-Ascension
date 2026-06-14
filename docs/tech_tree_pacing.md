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
| **Earth-start lab economy** | **~3,057 RP / sim-day** | 500 ResearchLab + 100 DataCenter + 10 AiCluster |
| Earth-start RP / sim-month | **~93,000 RP** | 3,057 × 30.4 |
| Earth-start vs 1-lab ratio | **~930×** | 3,057 ÷ 3.285 |
| Sim-day | 86,400 sim-seconds | `src/ui/time.rs` |
| Sim-year | 31,557,600 sim-seconds (Julian, 365.25 d) | `src/research/systems.rs:92` |
| Tick model | Continuous Euler integration, not discrete tick | `src/research/systems.rs:96-107, 181-198` |
| Time source | `SimulationTime.elapsed_seconds()` (no cap) | `src/ui/time.rs:58-63` |

A single ResearchLab is **2.4× the no-lab baseline**. With no lab the
player still accumulates 500 RP / sim-year, but at 1 lab the rate jumps
to 1,200 RP / sim-year. At Earth start the rate is **~930× a single
lab** — the LGD's pacing target assumes the Earth-start lab economy,
not 1-lab.

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

**Rationale.** The LGD's pacing target assumes the Earth-start lab
economy (~3,057 RP/sim-day) as the early-game reference, not 1-lab.
At Earth start, the hull-gate `chemical_spaceframes` should unlock in
20–90 sim-days (operator-selected Option C = 43.2 sim-days) so the
player can launch the first hull-gated probe within the first
sim-month, not the second. Tiers 2 – 5 are deliberately long —
fusion and antimatter are the long-horizon investments that
distinguish a 4X mid-game from late-game.

## Current state vs target

The `research_cost` values in `technologies.ron` were calibrated for
a *fleet-scale* lab economy (12+ labs), not the Earth-start lab economy
the LGD's pacing target was based on. At Earth start (500 ResearchLab +
100 DataCenter + 10 AiCluster = ~3,057 RP/sim-day), the original
tier-1 paid chain took **0.09 – 0.27 sim-days per tech** — far too
fast. The Coder's first PR (PR #166, divide-by-3) hit the opposite
problem: at 1 lab, the chain took 82 – 253 sim-days per tech, which is
1.4× the LGD target band at the cheap end and 1.4× over at the
expensive end.

**Option C rebalance (this PR).** Operator selected option C (20–90
sim-days at Earth start, moderate 4X pacing). The 16 tier-1 paid
`research_cost` values are multiplied by **110.05×** (rounded to the
nearest 5 RP), so the new range is **88,040 – 275,125 RP** with the
hull-gate `chemical_spaceframes` at **132,060 RP**. The relative cost
ratios between the 16 tier-1 paid techs are preserved exactly (each
tech's value is `orig × 110.05`, then rounded to 5 RP).

| Tech | Current cost (RP) | New cost (Option C) | Days at Earth start (3,057 RP/day) | Days at 1 lab (3.285 RP/day) | LGD target? |
|---|---:|---:|---:|---:|---|
| `project_management` (cheapest T1 paid) | 800 | 88,040 | 28.8 | 26,800 | ✓ within 20–90 band at Earth start |
| `orbital_mechanics` / `field_medicine` | 1,000 | 110,050 | 36.0 | 33,500 | ✓ |
| `chemical_spaceframes` (tier-1 hull gate) | 1,200 | 132,060 | 43.2 | 40,200 | ✓ hull gate in a single in-game season |
| `radio_astronomy` / `kevlar_armor` | 1,200 | 132,060 | 43.2 | 40,200 | ✓ |
| `advanced_composites` / `basic_automation` / `satellite_networks` / `conventional_missiles` / `hydroponics` | 1,500 | 165,075 | 54.0 | 50,200 | ✓ |
| `microelectronics` | 1,800 | 198,090 | 64.8 | 60,300 | ✓ |
| `high_energy_rocketry` / `closed_loop_ecology` / `survey_methodology` | 2,000 | 220,100 | 72.0 | 67,000 | ✓ |
| `solar_power` (T1 max) | 2,500 | 275,125 | 90.0 | 83,800 | ✓ at the top of the 20–90 band |
| Tier 1 paid total (16 techs) | 24,200 | 2,663,210 | 871 (≈ 1 sim-year) | 811,000 | — |

**At 1 lab (the LGD's pacing reference point),** the Option C values
take **26,800 – 83,800 sim-days per tech** — that's 73 – 230 sim-years
at 1 lab, which is deliberately unplayable. The 1-lab pacing target
only makes sense for a fresh colony that has not built any labs yet;
the LGD's real playtest assumption is the Earth-start lab economy
(3,057 RP/sim-day), where Option C delivers the 20–90 sim-day target
band for the full tier-1 sweep.

**Hull-gate timing at Earth start:** `chemical_spaceframes` unlocks in
**43.2 sim-days** — about 6 sim-weeks, a single in-game season. The 7
tier-1 hulls become buildable immediately after.

## Tier-scaling preservation

The hull-gate chain has 6 anchor costs that LGD wants to preserve
roughly 1:1 with hull class:

| Hull-gate | Tier | Current cost | New cost (Option C) |
|---|---:|---:|---:|
| `chemical_spaceframes` | 1 | 1,200 | 132,060 *(×110.05)* |
| `orbital_construction` | 2 | 8,000 | 8,000 *(unchanged)* |
| `orbital_assembly_heavy` | 2 | 18,000 | 18,000 *(unchanged)* |
| `carbon_nanotube_frames` | 3 | 24,000 | 24,000 *(unchanged)* |
| `fusion_superstructures` | 4 | 78,000 | 78,000 *(unchanged)* |
| `antimatter_containment_structures` | 5 | 125,000 | 125,000 *(unchanged)* |

Only the T1 hull-gate changes; the rest of the chain stays the same.
The relative gate positions for orbital construction (8k), fighter
frames (18k), CNT destroyers (24k), fusion cruisers (78k), and AM
keels (125k) are unchanged. The T1 → T2 jump is now 16.5×
(132,060 → 8,000) at the gate, but the player hits the T1 hull-gate
in 6 sim-weeks at Earth start, so the gap is felt as a smooth ramp
rather than a 110× wall.

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
plus the 100 DataCenter + 10 AiCluster stack earns roughly
**16 × 3.285 + 100 × ~0.274 + 10 × ~6.85 ≈ 124 RP / sim-day** extra
over the no-lab baseline, or **~45,300 RP / sim-year** from the
infrastructure. At that rate, the tier-2 median (4,750 RP) takes
**38 sim-days**, tier-3 median (17,000 RP) takes **137 sim-days**,
tier-4 median (65,000 RP) takes **524 sim-days**. The LGD pacing
assumes the player can plausibly operate 8 labs by tier-3; 16 labs
by tier-4. This is consistent with the building scale and the
available Industrial/Fusion/AI buildings.

## Reproduction note for QA

QA can validate this with the existing F12 debug menu:

1. Start a fresh game (do not skip the Earth-start lab economy).
2. Open the Research panel on Earth. Note the starting RP rate: the
   Earth-start stack (500 ResearchLab + 100 DataCenter + 10 AiCluster)
   yields **~3,057 RP/sim-day** (or **~93,000 RP/sim-month**).
3. Set time scale to `> 1 month / s` and observe `Research` resource
   accumulating at the Earth-start rate.
4. Queue `chemical_spaceframes` (now 132,060 RP). It should unlock in
   **43.2 ± 3 sim-days** at Earth start — about 6 sim-weeks, a single
   in-game season.
5. After `chemical_spaceframes` unlocks, confirm the 7 tier-1 hulls
   become buildable. None were buildable before.
6. Confirm the 12 baseline techs (`basic_physics`, `basic_chemistry`,
   `basic_biology`, etc.) remain auto-unlocked at game start exactly
   as before. None of the 16 paid tier-1 techs are pre-unlocked.

**Earth-unaffected check:** Earth's starting tech set and the 12
baseline `research_cost: 0.0` techs are byte-identical to pre-PR
main. Only the 16 tier-1 paid `research_cost` values changed.

The Coder's spawn-system PR (Phase A) can be tested independently of
the tech-tree PR: open a fresh game, observe the Day-1 fleet
constellation in Earth orbit (5 tier-1 hulls, no Venus / Jupiter /
Saturn / Alpha Centauri fleets), and confirm the Mars / Venus /
Jupiter / Saturn systems have **no** pre-spawned fleet.
