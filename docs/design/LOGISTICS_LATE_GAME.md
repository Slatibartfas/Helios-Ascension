# Late-Game Logistics — Design Intent (v0.4.x)

Design intent for the late-game logistics follow-ups listed in
[`ROADMAP.md`](../ROADMAP.md) under **v0.4.x — Late-Game Logistics Follow-ups**.

Owner: LGD. Status: design only. Rust delta scoping is CTO's job once this lands.

This document extends (does not replace) [`LOGISTICS_NETWORK.md`](LOGISTICS_NETWORK.md).
The 0.4.0 mechanics described there are the baseline; the items below are the
patches that turn that baseline into something a late-game empire can lean on.

---

## 1. Audit of the v0.4.x follow-up list

| # | Follow-up | Disposition | Notes |
|---|-----------|-------------|-------|
| 1 | Inter-system logistics | **Keep, but defer to 0.5+** | Couples to Exploration milestone; intra-system loop is not yet saturated for the typical 0.4.x player. See §3.1. |
| 2 | Shipping-capacity market | **Keep, 0.4.x scope** | A real market needs competing bidders. One-company saves do not exercise it. Lowest-cost 0.4.x lever; see §3.2. |
| 3 | Mega / Gigaton freighter hulls | **Keep, 0.4.2+ scope** | The ROADMAP cites `MEGA_GIGATON_FREIGHTER_TIERS.md` — that doc never landed. Reconstituting the design intent here. See §3.3. |
| 4 | Logistics network congestion / routing | **Strike from 0.4.x** | Lane capacity and priority preemption are 0.5+ systems-design work. Move to v0.5.x. |
| 5 | Outpost `ResourceRequest` auto-generation at founding | **Keep, 0.4.x scope** | The Phase 1 spec ships the trigger path; the per-building `ResourceRequest` chain is not yet end-to-end tested. Treat as a verification task, not new design. |
| 6 | Uranium default minimum for fission colonies | **Keep, 0.4.x scope** | One-line default in `MinimumStockpile` config; no design delta. |
| 7 | Company fleet icons on starmap | **Keep, 0.4.x scope** | Visualisation only; no new mechanic. Pairs with the Private Shipping overview panel. |
| 8 | Save / load | **Strike from this list** | Out of scope of the logistics scope. Belongs in its own ROADMAP line under v1.0.0. |
| 9 | Logistics panel as a top-level tab | **Keep, 0.4.x scope** | Promotes the Private Shipping overview subpanel. |
| 10 | `predict_build_effect` cost model rework | **Keep, 0.4.x scope — separate child** | The hardcoded food rate at the 0.4 boundary is a schema refactor, not a logistics item. Spawn as its own LGD child; do not block the logistics PR. |

**Net change to the ROADMAP follow-up list:** item 4 moves to 0.5.x, item 8 is
delisted from logistics scope, items 5/6/7/9 stay as scoped polish, item 10 is
spun out.

---

## 2. Highest-leverage follow-ups (the 3 to land first)

### 2.1 Shipping-capacity market (item 2)

**Why first:** the existing `ShippingCompany` AI is *already* bid-aware
(priority × distance × payment) — what's missing is a real market: more than one
company bidding, a transparent price ladder, and a way for the player to set
*who* wins. Without a market, the system has no friction and the player has no
strategic choice. With a market, the late-game loop emerges organically
(treasury pressure → company competition → bidding wars on Emergency
requests).

**Mechanic:** a `ContractAuction` per `ResourceRequest`. Open requests
broadcast to all companies. Each company submits a bid (price × ETA × cargo
available). The player can set the auction rule per body: **first-price
sealed-bid** (default) or **low-bid-wins**. Highest-priority requests use a
mandatory **second-price Vickrey** auction so Emergency freight isn't price-
gouged.

**UI hook:** a "Bid" column on the active-requests table, showing the
current high bid, the company name, and ETA. A "Reserve" toggle per request
reserves the cargo for the player to fulfil with their own fleet. A
per-colony auction-rule selector in the colony dossier.

**Balance levers:** bid spread is bounded by `max(0.5×, 2.0×)` of the
historical mean for that priority × distance bucket. This prevents a single
dominant company from runaway-pricing while still rewarding efficiency.

**Gating tech / era:** no new tech. The auctioneer lives in the `Economy`
plugin and is enabled by a `logistics_market: bool` setting (default **on**).

**Rust delta:** one new `ContractAuction` resource, one `AuctionRule` enum
(`FirstPrice | SecondPrice | LowBidWins`), one tick that walks open requests
and resolves expired auctions. RON-only for the per-bucket price band.

### 2.2 Mega / Gigaton freighter hulls (item 3)

**Why second:** this is the lift that makes "377 bodies" tractable. Even with
dozens of mid freighters, the per-trip Mt-on-transit math gets ugly when a
single asteroid mining outpost wants a 50 Mt iron top-up. A single Mega-class
freighter per route collapses the per-body cadence back to a manageable
clip.

**Mechanic:** two new hull sizes above the current `Small / Medium / Large`
ladder. Cargo capacity is the relevant axis, not hull tier. The
`required_tech` gate is the only construction barrier; `ConstructionMode` is
not extended (Mega-class freighters must be assembled at an Orbital Shipyard,
but the existing `OrbitalShipyard` mode already covers that — see
`src/shipbuilding/types.rs:194`).

| Hull | Default cargo | Isp assumption | required_tech | construction_mode | est. build points |
|------|---------------|----------------|---------------|-------------------|-------------------|
| Mega freighter | 1.02 Mt | NTR (900 s) | `ntr_cargo_frames` | OrbitalShipyard | 8,000 |
| Gigaton freighter | 1.00 Gt | gas-core / fusion-torch (≥10,000 s) | `gas_core_propulsion` | OrbitalShipyard | 60,000 |

The 1.02 Mt Mega figure matches the GRA-40 `FreighterTemplateRegistry` cargo
derivation rule (`cargo_bays × cargo_per_bay × tier_modifier`) — it is not a
magic constant. The 1.00 Gt Gigaton is a 1000× scale; AI companies do not
purchase Gigatons until the player's annual system-wide throughput exceeds
500 Gt/yr (see §3 open-question answers below).

**UI hook:** a "Mega" tag on the fleet list when a Mega/Gigaton freighter is
in the fleet. The Construction panel adds a "Tier filter: Standard | Mega |
Gigaton" segment. A tooltip on the cargo-bar shows "next Mega run: 17 days
(Mars Iron 800 Mt)" when a Mega is the assigned fulfiller.

**Balance levers:** Mega-class ships are **not** available to the player's
auto-build loop. Only the player can queue a Mega construction. Companies may
*hire* a Mega from a sister company for an Emergency request if the request
is life-support and the company has the treasury to afford it.

**Gating tech / era:** Tier 4 (NTR era) for Mega, Tier 6 (fusion-torch era)
for Gigaton. This dovetails with the propulsion-era rework (chemical → NTR →
gas-core). A player who jumps straight to fusion-torch may skip Mega.

**Rust delta:** one additive `SlotSize::XLarge` variant (claimed by the
original design comment; CTO to confirm the actual enum name) plus two
`ShipHullDefinition` RON entries. The capacity derivations in
`FreighterTemplateRegistry` already scale linearly; no new arithmetic.

### 2.3 Inter-system logistics scoping decision (item 1)

**Why third — and *not* a 0.4.x ship:** the inter-system loop pulls in
interstellar transfer windows, synodic periods between stars, the `Starmap`
graph, and the *Exploration* milestone (v0.5). Each of these is a multi-month
workstream. Trying to wedge it into 0.4.x as a "market only, no convoy routes"
half-feature will leak into the v0.5 design and we will pay for it twice.

**Disposition:** defer to 0.5+; keep the ROADMAP entry but add a one-line
"LGD holds the design intent; do not spec Rust work in 0.4.x" annotation.

---

## 3. Open questions — answers

These are the four questions raised at the bottom of the v0.4.x section in
`ROADMAP.md` lines 96–103.

### Q1. Should private shipping companies be opt-in or opt-out?

**Answer: opt-in, defaulting to *on* for new games and *off* for saves
imported from pre-0.4.0.**

Rationale: a player who has just shipped 0.4.0 and watched their carefully
managed construction queue get undercut by a private company's pricing will
assume the mechanic is broken. A player who started a fresh 0.4.0 game and
sees "Ares Freight Co. is delivering your starter Iron for 1,200 MC" will
treat it as part of the world. The asymmetry is the *existing player*. The
setting is exposed at the top-level Settings panel (not buried in a colony
dossier), so veteran players can flip it off in two clicks.

### Q2. At what empire size should the Mega / Gigaton hulls become available?

**Answer: tech-gated, not empire-size-gated.**

Mega: requires `ntr_cargo_frames` tech (Tier 4).
Gigaton: requires `gas_core_propulsion` tech (Tier 6).

The original GRA-45 design proposed an "annual throughput" gate (500 Gt/yr)
*in addition* to the tech gate. LGD withdraws that proposal. Empire-size
gates are invisible to the player until they trip, then they feel arbitrary.
Tech gates are visible, researchable, and reward the propulsion-era arc. If
balance testing shows the Gigaton being unlocked too early, the lever is a
tech-prereq change, not a hidden empire-size threshold.

The AI "may hire a Mega on Emergency" rule (see §2.2 balance levers) is the
*only* size-conditioned behaviour, and it triggers on a per-request basis
(treasury check at bid time), not on an empire-size threshold.

### Q3. Bidding wars vs first-come-first-served?

**Answer: bidding for Emergency and Construction priorities;
first-come-first-served for Maintenance and Trade.**

Bidding is expensive UI. Maintenance runs (top-up the colony's minimum stock)
are not strategic — the player just wants the Iron there. FCFS keeps the
panel quiet. Construction and Emergency are the strategic layers: a player
who is mid-build of a *Ringworld Habitat Module* will pay 4× to win the bid
because the delay is more expensive than the credits. Trade is a stretch
goal that lands later — for 0.4.x, leave Trade at FCFS and revisit when the
inter-system market lands in 0.5.

### Q4. Is inter-system logistics a 0.4.x patch or a 0.5+ feature?

**Answer: 0.5+, no exceptions.**

The inter-system loop is a sibling of the Exploration milestone (v0.5) and
the market layer (v0.7). To land it in 0.4.x would require (a) a transfer-
window model between stars, (b) a `Starmap` graph node type for the convoy
route, (c) a treasury escrow mechanic for the cross-star sale, and (d) UI
for the convoy route list. Each of these is a multi-PR workstream. Keep
0.4.x focused on *intra-system hardening*: market, Mega/Gigaton, default
minimums, panel promotion, icon visibility.

---

## 4. Items that should NOT land in 0.4.x

These are the items the ROADMAP lists under v0.4.x that LGD is explicitly
moving out. The reasoning is in the disposition table in §1.

| Item | Move to | Why |
|------|---------|-----|
| Logistics network congestion / routing (item 4) | v0.5.x | Systems-design work; depends on the convoy model that lands with inter-system in 0.5. |
| Save / load (item 8) | v1.0.0 (its own line) | Not a logistics item. Will be tracked in the v1.0 milestone once a `Save` plugin is scoped. |
| `predict_build_effect` cost model rework (item 10) | 0.4.x but **separate child issue** | Schema refactor, not a logistics item. Spawn as a child of the 0.4.x list with its own design pass. |

---

## 5. What this doc commits LGD to

- Reconcile the ROADMAP v0.4.x list with the dispositions in §1.
- Land the Market + Mega/Gigaton design intent as a CTO hand-off for
  Rust-side scoping.
- Defer inter-system + congestion + save/load out of 0.4.x, with a note in
  the ROADMAP.
- Spawn the `predict_build_effect` child issue for the cost-model schema
  refactor.

## 6. What this doc does NOT commit LGD to

- Any specific price formula, balance number, or UI layout. Those go in the
  Rust hand-off once the CTO scopes the children.
- Mega/Gigaton build-point tuning. The 8,000 / 60,000 figures are estimates;
  the Rust impl should derive from existing `FreighterTemplateRegistry` rules
  and the 50–100× per-build scale bar set in GRA-22.
- Inter-system or save/load design. Out of scope for 0.4.x.
