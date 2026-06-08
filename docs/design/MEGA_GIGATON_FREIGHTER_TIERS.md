# Mega / Gigaton Freighter Tiers — Design Intent

Design intent for the Mega- and Gigaton-class freighter hulls, parked for
v0.4.2+ implementation. Extracted from
[`LOGISTICS_LATE_GAME.md`](LOGISTICS_LATE_GAME.md) §2.2 so this file is a
self-contained reference and the `ROADMAP.md` link resolves.

Status: design only. Rust delta scoping is CTO's job once the implementation
PR is spawned.

---

## Why

Even with dozens of mid freighters, the per-trip Mt-on-transit math gets
ugly when a single asteroid mining outpost wants a 50 Mt iron top-up. A
single Mega-class freighter per route collapses the per-body cadence back
to a manageable clip. A Gigaton-class freighter is the inter-system
equaliser once the 0.5+ market layer lands.

The 0.4.0 cargo ladder (light / mid / heavy) tops out around 1.5–2 kt per
hull slot. Late-game requests routinely exceed 50 kt. The Mega/Gigaton
tiers are the lift that makes "377 bodies" tractable.

---

## Tier table

| Hull | Default cargo | Isp assumption | required_tech | construction_mode | est. build points |
|------|---------------|----------------|---------------|-------------------|-------------------|
| Mega freighter | 1.02 Mt | NTR (900 s) | `ntr_cargo_frames` | OrbitalShipyard | 8,000 |
| Gigaton freighter | 1.00 Gt | gas-core / fusion-torch (≥10,000 s) | `gas_core_propulsion` | OrbitalShipyard | 60,000 |

The 1.02 Mt Mega figure matches the GRA-40 `FreighterTemplateRegistry`
cargo derivation rule (`cargo_bays × cargo_per_bay × tier_modifier`) — it
is not a magic constant. The 1.00 Gt Gigaton is a 1000× scale; AI companies
do not purchase Gigatons until the inter-system market lands in 0.5+.

The build-point estimates (8,000 / 60,000) obey the 50–100× per-build scale
bar set in GRA-22. The Rust impl should derive from the existing
`FreighterTemplateRegistry` rules and not hardcode these figures.

---

## Construction gate

`required_tech` is the only construction barrier. `ConstructionMode` is
**not** extended — Mega/Gigaton freighters must be assembled at an Orbital
Shipyard, but the existing `OrbitalShipyard` variant on
`src/shipbuilding/types.rs:194` already covers that.

| Hull | Surface launch? | Orbital assembly? | Orbital shipyard? |
|------|-----------------|-------------------|-------------------|
| Mega freighter | No | No | Yes |
| Gigaton freighter | No | No | Yes |

The construction site itself (which shipyard entity) is picked at build
time by the existing site-selection logic — no new `ConstructionMode`
variant is needed.

---

## Era gating

| Hull | Era | Propulsion tier | Tech tier |
|------|-----|-----------------|-----------|
| Mega freighter | NTR era | Tier 4 | Tier 4 |
| Gigaton freighter | gas-core / fusion-torch era | Tier 6 | Tier 6 |

A player who jumps straight to fusion-torch may skip Mega by going
straight to Gigaton. The Mega tier is the "right-sized" late-NTR workhorse;
the Gigaton tier is the gas-core flagship.

---

## AI behaviour

- **Player:** may queue a Mega or Gigaton construction from the Construction
  panel. Standard player controls.
- **AI companies:** may **hire** a Mega from a sister company for an
  Emergency request if the request is life-support *and* the company has
  the treasury to afford it. AI companies may **not** own a Mega.
  Gigatons are not available to AI at all in 0.4.x.

The "AI may hire a Mega" rule is the only size-conditioned behaviour, and
it triggers on a per-request basis (treasury check at bid time), not on an
empire-size threshold. The original GRA-45 design proposed an
"annual throughput" gate (500 Gt/yr) in addition to the tech gate. LGD
withdraws that proposal — see
[`LOGISTICS_LATE_GAME.md`](LOGISTICS_LATE_GAME.md) §3 Q2 for the reasoning.

---

## UI hooks

- A "Mega" / "Gigaton" tag on the fleet list when one of these hulls is
  in the fleet.
- The Construction panel adds a "Tier filter: Standard | Mega | Gigaton"
  segment.
- A tooltip on the resource bar shows "next Mega run: 17 days (Mars Iron
  800 Mt)" when a Mega is the assigned fulfiller.
- The Private Shipping overview panel filters by hull tier; AI hire-Mega
  events surface as a "Sister-company assist" line in the bid log.

---

## Rust delta (hand-off to CTO)

1. One additive enum variant in the slot-size enum (claimed by the original
   design comment; CTO to confirm the actual enum name) — e.g.
   `SlotSize::XLarge`. This is a single match-arm addition; the existing
   arms continue to behave as today.
2. Two new `ShipHullDefinition` RON entries under `assets/data/ship_hulls.ron`:
   - `mega_freighter_frame`
   - `gigaton_freighter_frame`
3. Two new `FreighterTemplate` RON entries under the GRA-40
   `FreighterTemplateRegistry`:
   - `mega_freighter` (uses `ntr_cargo_frames` + `mega_freighter_frame`)
   - `gigaton_freighter` (uses `gas_core_propulsion` + `gigaton_freighter_frame`)
4. The capacity derivations in `FreighterTemplateRegistry` already scale
   linearly; no new arithmetic. The `cargo_capacity_t` query function
   works as-is.
5. The AI hire-Mega rule is a single new branch in
   `src/economy/company.rs:resolve_bid` (or wherever the bid-decision
   function lives in the post-GRA-43 codebase) gated on
   `request.priority == Emergency` and `company.treasury > mega_hire_cost`.

There is **no** new `ConstructionMode` variant. There is **no** new
`ResourceType`. The freight capacity is bounded by the existing
`cargo_per_bay` × `bays` × `tier_modifier` rule.

---

## Test plan

- Build a Mega freighter at an Orbital Shipyard. Verify the build is
  blocked at `ntr_cargo_frames` is not researched.
- Queue a Construction-priority 800 Mt iron request. Assign a Mega
  freighter. Verify a single delivery closes the request and the cargo
  bar shows 0 Mt in transit on completion.
- Hand a save with a Gigaton under construction to a player who has not
  researched `gas_core_propulsion`. Verify the construction halts with a
  "missing tech" UI error (not a soft fail).

---

## Out of scope

- Inter-system Megatons (different design; see 0.5+ inter-system
  logistics).
- AI-owned Megatons (no business case; AI hire-Mega is sufficient).
- Build-point re-tuning for the existing light/mid/heavy ladder (separate
  GRA-45 follow-up; not part of this tier addition).
