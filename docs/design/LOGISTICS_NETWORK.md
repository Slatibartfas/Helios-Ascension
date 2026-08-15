# Logistics Network Design

Design specification for the intra-system resource transport system (v0.4.x).

Inspired by **Aurora 4X** (player-directed logistics) and **Distant Worlds 2** (private sector automation).

## Status

| Phase | Feature | Status | Implementation |
|-------|---------|--------|----------------|
| 1 | Resource locality (`LocalStockpile` per body, no system-pool fallback) | ✅ Shipped (v0.4.x) | `src/economy/components.rs`, `src/colony/systems.rs`, `src/economy/budget.rs` |
| 1 | `ResourceRequest` + `PendingResourceRequests`; construction emits requests when local is short | ✅ Shipped (v0.4.x) | `src/economy/logistics.rs`, `src/colony/systems.rs` |
| 1 | Construction panel shows "Waiting for freighter" / "Awaiting resources" | ✅ Shipped (v0.4.x) | `src/ui/construction/queue.rs` + `src/ui/construction/tooltip.rs` — bevy_ui path. The legacy `src/ui/construction_panel.rs` and the canary-era `src/ui/construction.rs` were deleted in v0.5.2; the new construction UI was **refactored** into the `src/ui/construction/` directory |
| 2 | Fleet panel — assign a player Freighter to an open request | ✅ Shipped (v0.4.x) | `src/ui/fleets_panel.rs` ([PR #99](https://github.com/Slatibartfas/Helios-Ascension/pull/99)) |
| 3 | Private `ShippingCompany` AI + payment + buy-ship loop | ✅ Shipped (v0.4.x) | `src/economy/company.rs` |
| 4 | `MinimumStockpile` per colony + default Life Support thresholds + per-tick system | ✅ Shipped (v0.4.x) | `src/economy/logistics.rs` |

The doc below describes the design that was implemented. Section §[Resource Locality](#resource-locality) and §[Implementation Roadmap](#implementation-roadmap) mark which items are now in production.

---

## Table of Contents

1. [Design Goals](#design-goals)
2. [Resource Locality](#resource-locality)
3. [Resource Requests](#resource-requests)
4. [Fulfilling Requests — Player Fleets](#fulfilling-requests--player-fleets)
5. [Fulfilling Requests — Private Shipping Companies](#fulfilling-requests--private-shipping-companies)
6. [Minimum Stockpile Settings](#minimum-stockpile-settings)
7. [UI Design](#ui-design)
8. [Implementation Roadmap](#implementation-roadmap)
9. [Data Structures](#data-structures)

---

## Design Goals

| Goal | Description |
|------|-------------|
| **Realism** | Resources are physical objects that must be moved by ships — even within a solar system |
| **Strategic depth** | Players choose between manual control (Aurora-style) and delegation to AI companies (DW2-style) |
| **Emergence** | Private companies grow organically, creating visible economic activity in the system |
| **Accessibility** | New players can hand off logistics entirely to companies; veterans can micro-manage every cargo run |
| **Scalability** | The system must work from a 2-body Sol early game to a 377-body late-game empire |

---

## Resource Locality

### Current state (v0.3)

Resources are pooled **system-wide** in a `ContextualStockpile`.  Construction on any body draws from any other body in the same system without requiring physical transport.  Only interstellar supply requires a Freighter fleet.

### Current state (v0.4+ — Shipped)

Each body (planet, moon, asteroid, station) has its own **local stockpile** (`LocalStockpile` component in `src/economy/components.rs`).  Resources are **physically located** on a specific body and must be carried by a ship to be used elsewhere.  Construction draws **only** from the destination body's `LocalStockpile`; when local materials are short the construction system publishes a `ResourceRequest` and the project is set to `awaiting_resources` until delivery arrives.

**Aggregated display is retained**: the `ContextualStockpile` resource (built every frame by `update_contextual_stockpile` in `src/economy/budget.rs`) sums stockpiles for the current view scope — bodies in the active star system when in **System view**, every body across all systems when in **Starmap view** — and is used by the top resource bar and the Economy panel.  Construction does **not** read this resource; it is display-only.

---

## Resource Requests

Whenever a body needs resources it cannot produce locally, it **publishes a resource request**.  This replaces the current silent "draw from system pool" behaviour.

### Triggers

| Event | Resource request created |
|-------|--------------------------|
| Building queued | Materials needed to construct the building (Iron, Silicates, etc.) |
| Outpost founded | Full starter-package materials at the destination body |
| Minimum stockpile below threshold | Replenishment run to reach the configured minimum |
| Life support shortfall | Emergency O₂ or Water resupply |

### Request fields (shipped)

```rust
struct ResourceRequest {
    id: u64,
    destination_body: Entity,       // where resources should go
    destination_name: String,       // for UI display
    resource: ResourceType,
    amount_mt: f64,                 // megatonnes requested
    priority: RequestPriority,      // Emergency > Construction > Maintenance > Trade
    state: RequestState,            // lifecycle (see below)
    in_transit_mt: f64,             // ≤ amount_mt
    eta_seconds: Option<f64>,       // SimulationTime arrival
    assigned_company_idx: Option<usize>,
    created_at_seconds: f64,
    source_body: Option<Entity>,
    linked_project: Option<Entity>, // ConstructionProject blocked on this request
    payment_made: bool,
    completed_at_seconds: Option<f64>,
}

enum RequestPriority { Trade, Maintenance, Construction, Emergency }
enum RequestState    { Pending, Assigned, InTransit, Delivered, Expired }
```

Full type lives in `src/economy/logistics.rs`; the live collection is the
`PendingResourceRequests` ECS `Resource` (initialised in `src/colony/systems.rs`).

### Request lifecycle

```
Created → Pending → Assigned → InTransit → Delivered
                 ↘ Expired (if ship never assigned in time)
```

State transitions are driven by the `ShippingCompany` AI (`src/economy/company.rs`)
for automated fulfilment and by the Fleet panel (manual assignment) for player
freighters.  The `complete_deliveries` system credits the destination
`LocalStockpile` on arrival and unblocks the linked `ConstructionProject` when
all linked requests are `Delivered`.

---

## Fulfilling Requests — Player Fleets

Players can fulfil any open request manually using Freighter fleets:

1. Open the **Fleet** panel and select a Freighter fleet.
2. Open the **Transfer Planner** and set the destination.
3. The planner shows any open resource requests at the destination and lets the player assign the fleet to a specific request.
4. On arrival the cargo is unloaded and the request marked as delivered.

This is the *Aurora 4X*-style approach: total manual control at the cost of player attention.

---

## Fulfilling Requests — Private Shipping Companies

Private shipping companies operate autonomously.  They are **not player-controlled** but respond to the market of resource requests.

### Company attributes (planned)

| Attribute | Description |
|-----------|-------------|
| `name` | Procedurally generated company name |
| `fleet` | List of freighter ships owned (grows over time) |
| `treasury` | Credits available to buy ships and pay fuel |
| `reputation` | Reliability score — affects which contracts they win |

### AI behaviour loop

Every simulation tick (configurable interval, e.g. 1 in-game day):

1. **Scan open requests** sorted by priority × payment.
2. **Assign nearest available freighter** to the highest-value request it can physically reach within its fuel budget.
3. **Execute Hohmann/fast transfer** (same mechanics as player fleets — uses `orbital_mechanics.rs`).
4. **Deliver cargo** — deducts from source body's `LocalStockpile`, adds to destination.
5. **Collect payment** — credits transferred from `GlobalBudget` to company treasury.
6. **Expand fleet** — when treasury exceeds a threshold, purchase a new ship at a shipyard.

### Payment formula (planned)

```
payment = base_rate_per_mt × amount × distance_au × priority_multiplier
```

| Priority | Multiplier |
|----------|------------|
| Emergency | 4.0× |
| Construction | 2.0× |
| Maintenance | 1.0× |
| Trade | 0.5× |

### Starting state

At game start, **one private shipping company** exists with a small fleet of 2–3 chemical-propulsion freighters based at Earth.  As the player's economy grows, more companies emerge and existing ones expand.

Companies are visible on the starmap as small fleet icons and their routes can be traced by hovering.

---

## Minimum Stockpile Settings

Players can configure a **minimum stockpile** for each resource on each colony.  When the local stockpile drops below this threshold, a Maintenance-priority resource request is automatically created.

This is the core "set-and-forget" feature (Distant Worlds 2 style):

- Set Mars minimum Iron = 5,000 Mt → a freighter is dispatched automatically whenever Mars iron drops below 5,000 Mt.
- Set Moon minimum Uranium = 500 Mt → the Moon is always topped up for its Fission Reactors.
- Emergency thresholds: O₂ and Water get default minimums on all colonies with Life Support.

### UI (planned)

In the colony dossier panel, each resource row will have:

```
[Resource icon]  Iron          Available: 12,450 Mt
                               Min stockpile: [___5000___] Mt  [Clear]
                               (Freighter assigned — ETA 47 days)
```

- Clicking "Clear" removes the minimum, cancelling any pending Maintenance request.
- The ETA label shows the in-transit amount and arrival time.

---

## UI Design

### Logistics Panel (new tab, planned)

A new top-level panel tab **Logistics** showing:

**Active Requests** table:
| Colony | Resource | Amount | Priority | Status | ETA |
|--------|----------|--------|----------|--------|-----|
| Mars | Iron | 2,000 Mt | Construction | In transit (Ares Freight Co.) | 47 days |
| Ceres | Uranium | 100 Mt | Maintenance | Pending | — |

**Company Registry** table:
| Company | Ships | Routes | Credits |
|---------|-------|--------|---------|
| Ares Freight Co. | 3 freighters | 2 active | 420K MC |
| Solar Carriers | 1 freighter | 1 active | 90K MC |

**Per-colony Minimum Stockpile** settings (reachable via colony dossier or the Logistics panel).

### Resource Bar changes

The top resource bar continues to show **system-wide aggregates** for at-a-glance view.  An **in-transit** count is shown in amber when resources are en route but not yet landed:

```
  Iron  45,230 Mt  (+1,200 in transit)
```

---

## Implementation Roadmap

### Phase 1 — Resource locality (v0.4.2) ✅ Shipped
- [x] Stop construction from reading `ContextualStockpile`; read destination body `LocalStockpile` only
- [x] Create `ResourceRequest` component and `PendingResourceRequests` resource
- [x] Generate construction requests when a building is queued and resources aren't locally available
- [x] Show open requests in construction panel ("Waiting for: Iron 500 Mt")

### Phase 2 — Player-directed transport (v0.4.3) ✅ Shipped
- [x] Fleet panel lists open resource requests at each body
- [x] Assign a Freighter fleet to a request from the Fleet panel
- [x] Fleet arrival auto-delivers cargo and closes the request

> Shipped in [PR #99](https://github.com/Slatibartfas/Helios-Ascension/pull/99) — `assignee_fleet_id` on `ResourceRequest`, `process_fleet_logistics_assignments` system, Logistics section in the Fleet panel.

### Phase 3 — Private shipping companies (v0.4.4) ✅ Shipped
- [x] `ShippingCompany` ECS resource (Vec of companies)
- [x] AI tick: scan requests → assign nearest freighter → execute transfer
- [x] Payment system: deduct from `GlobalBudget`, add to company treasury
- [x] Company fleet expansion: buy ships at shipyards when treasury threshold met
- [x] UI: company registry in Logistics panel, fleet icons on starmap

### Phase 4 — Minimum stockpile (v0.4.5) ✅ Shipped
- [x] `MinimumStockpile` component: `HashMap<ResourceType, f64>` per colony
- [x] System generates Maintenance requests when below threshold
- [x] Default minimums for Life Support resources (O₂ 200 Mt, Water 100 Mt — PR #97; Uranium deferred)
- [x] UI in construction panel: per-resource minimum input fields (`render_minimum_stockpile_editor` was **reimplemented** in `src/ui/construction/state.rs` / `src/ui/construction/data.rs` after the v0.5.2 split; the legacy `src/ui/construction_panel.rs:2473` no longer exists)

---

## Data Structures

The types below are the **shipped** shapes, lifted from the implementation files
so the design doc and the code stay in lockstep.  See the `// src/...` links for
the canonical definition.

```rust
// src/economy/components.rs

#[derive(Component, Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalStockpile {
    pub stockpiles: HashMap<ResourceType, f64>,  // Mt
}

// src/economy/budget.rs

#[derive(Resource, Debug, Clone, Default)]
pub struct ContextualStockpile {
    pub stockpiles: HashMap<ResourceType, f64>,         // view-scoped sum
    pub context_label: String,                         // "Sol System" / "All Systems"
    pub active_system_id: Option<usize>,
}

// src/economy/logistics.rs

#[derive(Component, Default, Clone, Debug)]
pub struct MinimumStockpile {
    pub thresholds: HashMap<ResourceType, f64>,  // Mt
}

#[derive(Debug, Clone)]
pub struct ResourceRequest {
    pub id: u64,
    pub destination_body: Entity,
    pub destination_name: String,
    pub resource: ResourceType,
    pub amount_mt: f64,
    pub priority: RequestPriority,
    pub state: RequestState,
    pub in_transit_mt: f64,
    pub eta_seconds: Option<f64>,
    pub assigned_company_idx: Option<usize>,
    pub created_at_seconds: f64,
    pub source_body: Option<Entity>,
    pub linked_project: Option<Entity>,
    pub payment_made: bool,
    pub completed_at_seconds: Option<f64>,
}

#[derive(Default, Resource)]
pub struct PendingResourceRequests {
    pub requests: Vec<ResourceRequest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RequestPriority { Trade, Maintenance, Construction, Emergency }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestState { Pending, Assigned, InTransit, Delivered, Expired }

// src/economy/company.rs

#[derive(Resource, Default)]
pub struct ShippingCompanies {
    pub companies: Vec<ShippingCompany>,
}

pub struct ShippingCompany {
    pub name: String,
    pub fleet: Vec<CompanyFreighter>,
    pub treasury_mc: f64,
    pub reputation: f32,  // 0.0–1.0
}

pub struct CompanyFreighter {
    pub entity: Entity,
    pub cargo_capacity_mt: f64,
    pub current_request: Option<usize>,
}
```

---

## References

- **Aurora 4X**: Manual freight assignments, player-managed supply lines
- **Distant Worlds 2**: Private sector handles most logistics; player sets policies and minimum stocks
- `src/economy/components.rs` — `LocalStockpile` (per-body stockpile)
- `src/economy/budget.rs` — `ContextualStockpile` (view-scoped aggregate, display-only)
- `src/economy/logistics.rs` — `ResourceRequest`, `PendingResourceRequests`, `MinimumStockpile`, request-flow systems
- `src/economy/company.rs` — `ShippingCompany` AI, payment, fleet expansion
- `src/colony/systems.rs` — `process_construction_actions` (emits `ResourceRequest` when local is short)
- `src/ui/construction/queue.rs` + `src/ui/construction/tooltip.rs` — bevy_ui path; "Awaiting resources" / "Waiting for freighter" badges. The legacy `src/ui/construction_panel.rs` and the canary-era `src/ui/construction.rs` were deleted in v0.5.2; the new construction UI was **refactored** into the `src/ui/construction/` directory
- `src/ui/fleets_panel.rs` — manual freighter → request assignment
- `src/fleets/orbital_mechanics.rs` — transfer physics reused by company ships
