# Logistics Network Design

Design specification for the intra-system resource transport system (v0.4.x).

Inspired by **Aurora 4X** (player-directed logistics) and **Distant Worlds 2** (private sector automation).

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
9. [Data Structures (Planned)](#data-structures-planned)

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

### Planned state (v0.4+)

Each body (planet, moon, asteroid, station) has its own **local stockpile** (`LocalStockpile` component).  Resources are **physically located** on a specific body and must be carried by a ship to be used elsewhere.

**Aggregated display is retained**: the Economy panel and resource bar still show *system-wide totals* so the player can see the big picture.  Individual body stockpiles are visible in the Survey / dossier panel.

> **Note:** The `ContextualStockpile` aggregation (used for display) already exists in the codebase.  The change is to **stop using it as a construction input** — construction will draw from the destination body's local stockpile only.

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

### Request fields (planned)

```rust
struct ResourceRequest {
    destination_body: Entity,       // where resources should go
    resource: ResourceType,
    amount_mt: f64,                 // megatonnes requested
    priority: RequestPriority,      // Emergency > Construction > Maintenance > Trade
    expires_at: Option<f64>,        // SimulationTime; None = persistent
    fulfilled_by: Option<Entity>,   // fleet or company assigned
}

enum RequestPriority {
    Emergency,      // life support failure — highest price paid
    Construction,   // building queue blocked
    Maintenance,    // minimum stockpile below threshold
    Trade,          // profitable surplus/deficit balancing
}
```

### Request lifecycle

```
Created → Pending → Assigned → In-Transit → Delivered → Closed
                 ↘ Expired (if ship never assigned in time)
```

Open requests are visible in the **Logistics panel** (planned) and can be manually assigned to player fleets.

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

### Phase 1 — Resource locality (v0.4.2)
- [ ] Stop construction from reading `ContextualStockpile`; read destination body `LocalStockpile` only
- [ ] Create `ResourceRequest` component and `PendingResourceRequests` resource
- [ ] Generate construction requests when a building is queued and resources aren't locally available
- [ ] Show open requests in construction panel ("Waiting for: Iron 500 Mt")

### Phase 2 — Player-directed transport (v0.4.3)
- [ ] Fleet panel lists open resource requests at each body
- [ ] Assign a Freighter fleet to a request from the Fleet panel
- [ ] Fleet arrival auto-delivers cargo and closes the request

### Phase 3 — Private shipping companies (v0.4.4)
- [ ] `ShippingCompany` ECS resource (Vec of companies)
- [ ] AI tick: scan requests → assign nearest freighter → execute transfer
- [ ] Payment system: deduct from `GlobalBudget`, add to company treasury
- [ ] Company fleet expansion: buy ships at shipyards when treasury threshold met
- [ ] UI: company registry in Logistics panel, fleet icons on starmap

### Phase 4 — Minimum stockpile (v0.4.5)
- [ ] `MinimumStockpile` component: `HashMap<ResourceType, f64>` per colony
- [ ] System generates Maintenance requests when below threshold
- [ ] Default minimums for Life Support resources (O₂, Water, Uranium)
- [ ] UI in colony dossier: per-resource minimum input fields with ETA display

---

## Data Structures (Planned)

```rust
// economy/logistics.rs (new file)

#[derive(Component, Default)]
pub struct MinimumStockpile {
    pub thresholds: HashMap<ResourceType, f64>,  // Mt
}

#[derive(Component)]
pub struct ResourceRequest {
    pub destination_body: Entity,
    pub resource: ResourceType,
    pub amount_mt: f64,
    pub priority: RequestPriority,
    pub expires_at: Option<f64>,
    pub fulfilled_by: Option<Entity>,
    pub in_transit_mt: f64,
}

#[derive(Default, Resource)]
pub struct PendingResourceRequests {
    pub requests: Vec<ResourceRequest>,
}

// economy/company.rs (new file)

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
- `src/economy/components.rs` — `LocalStockpile` (existing per-body stockpile)
- `src/economy/budget.rs` — `ContextualStockpile` (existing aggregation, retained for display)
- `src/colony/systems.rs` — `process_construction_actions` (current same-system draw, to be changed)
- `src/fleets/orbital_mechanics.rs` — transfer physics (company ships reuse same code)
