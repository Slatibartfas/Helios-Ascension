# Colonies, Buildings & Resource Transport

A complete player reference for founding colonies, constructing buildings, and supplying them with resources.

---

## Table of Contents

1. [How Colonies Work](#how-colonies-work)
2. [Founding a New Colony (Establish Outpost)](#founding-a-new-colony-establish-outpost)
3. [Transporting Resources to a New Colony](#transporting-resources-to-a-new-colony)
4. [Buildings Reference](#buildings-reference)
   - [Infrastructure](#infrastructure)
   - [Industry](#industry)
   - [Logistics](#logistics)
   - [Power](#power)
   - [Population & Food](#population--food)
   - [Research](#research)
   - [Financial & Commerce](#financial--commerce)
   - [Military & Shipbuilding](#military--shipbuilding)
5. [Building Scale & Design Philosophy](#building-scale--design-philosophy)
6. [Construction Workflow](#construction-workflow)
7. [Population Growth](#population-growth)
8. [Logistics Efficiency](#logistics-efficiency)
9. [Debug / Cheat Controls](#debug--cheat-controls)

---

## How Colonies Work

Each colonised body has a `Colony` component that tracks:

- **Population** — number of residents (millions to billions for Earth-tier worlds)
- **Buildings** — a list of constructed buildings by type and count
- **Construction queue** — buildings currently being built, with build-progress in Build Points (BP)
- **Housing capacity** — the total number of residents the colony can house
- **Food production / consumption** — megatonnes per year; a deficit slows growth
- **Logistics efficiency** — determines how effectively mines, labs, etc. operate

Resource stockpiles are stored in `LocalStockpile` (a separate ECS component) and scoped to the body.  Construction draws from the **same-system stockpile pool** — any resource on any body within the same star system can fund construction on another body in that system.

---

## Founding a New Colony (Establish Outpost)

### Step 1 — Select the target body

Open the **Survey** tab and click a body in the body list or directly in the 3D view.  The right-hand dossier panel will show detailed information about the selected body.

### Step 2 — Check habitability

The dossier shows an **Establish Outpost** button near the bottom of the panel.  Two **hard blocks** prevent founding:

| Condition | Reason |
|-----------|--------|
| Gas Giant | No solid surface — outpost impossible |
| Surface gravity > 3 g | Exceeds human physiological limits |

An **amber warning** (⚠) is shown if the colony-cost score is high (harsh environment), but you can still found there.

### Step 3 — Review the starter package

Every new outpost receives a starter set of buildings **automatically queued**:

| Building | Qty | Notes |
|----------|-----|-------|
| Life Support | ×1 | Required on bodies without a breathable atmosphere |
| Housing Complex | ×1 | +25M housing capacity |
| Fission Reactor | ×2 | ~40 GW power; fuelled by Uranium |
| Agricultural Dome | ×2 | +8 Mt/yr food (~80K people) |

### Step 4 — Check ongoing environmental costs

The dossier lists per-person-per-year running costs *before* you click the button:

| Resource | Rate | When |
|----------|------|------|
| 💧 Water | 50 t/person/yr | Always (recycling losses) |
| 🫁 Oxygen | 100 t/person/yr | Bodies without a breathable atmosphere |

### Step 5 — Send resources first

> **⚠ Important**: Resources must arrive at the target body *before* construction can start.  
> Starter buildings are queued immediately, but they will remain paused until the required materials are present in the local system stockpile.

See [Transporting Resources to a New Colony](#transporting-resources-to-a-new-colony) for how to do this.

### Step 6 — Click "🏗  Establish Outpost"

The button enqueues the outpost request.  On the next simulation tick the starter buildings are queued on the new colony.  Switch to the **Construction** tab, select the new colony from the colony dropdown, and watch progress.

---

## Transporting Resources to a New Colony

New colonies start with **zero local stockpile**.  All starter-building materials (Iron, Silicates, Uranium, etc.) must be shipped in from an existing colony or mining site in the same — or a different — star system.

### Within the same star system

Resources are pooled **system-wide**.  Any stockpile on any body in the same star system counts toward construction on every other body in that system.  No explicit freighter action is needed once resources exist anywhere in the system.

**Workflow:**

1. Ensure Earth (or another established colony) has sufficient Iron, Silicates, Uranium, etc.
2. Found the new outpost — construction will draw from the system pool automatically.
3. If stocks run low, build more Mines and Refineries on resource-rich bodies nearby.

### From another star system (interstellar supply run)

You need a **Freighter** fleet.

1. **Open the Fleet panel** and spawn or select a Freighter fleet at the origin colony.
2. **Open the Transfer Planner** (in the Fleet panel) and set the destination body.
3. Choose a transfer option (Efficient / Moderate / Fast); efficient Hohmann burns use the least fuel.
4. The fleet travels along the computed Keplerian arc; use *phased departure* to align with the optimal transfer window.
5. On arrival the fleet's cargo is automatically added to the destination body's local stockpile.

> **Tip:** The in-game resource bar shows how much of each resource is available in the current system.  Switch to Starmap view to see system-wide aggregates.

### Resources needed for the starter buildings

The following materials are drawn from the system stockpile when a typical outpost is founded:

| Material | Approx. amount needed |
|----------|-----------------------|
| Iron | Structural framework |
| Silicates | Dome glass & insulation |
| Aluminum | Lightweight structures |
| Uranium | Fission reactor fuel rods |
| Carbon | Composite reinforcement |

Exact amounts are shown on each building card in the Construction panel (need / available).

---

## Buildings Reference

The game has **47 building types** across **8 categories**.  Each building card in the Construction panel shows:

- 🏗 Name + icon
- Description (what it is)
- ▸ **Effect lines** — the actual numeric impact per building
- BP / 👷 Workforce / ⚡ Power demand
- ⏱ Estimated build time
- Resource costs (current stock vs. required)

Building outputs below are **per building**.

---

### Infrastructure

| Building | Effect | BP | Workers | Notes |
|----------|--------|----|---------|-------|
| 🏙 Housing Complex | **+25M housing capacity** | 200 | 500 | Habitable worlds only |
| 🏠 Habitat Dome | **+50M housing capacity** | 800 | 1,000 | Pressurised; any body |
| ⛏ Underground Habitat | **+30M housing capacity** | 1,200 | 1,500 | Buried; ideal for airless worlds |
| 🌬 Life Support | Enables habitation on vacuum/hostile worlds; recycles air & water | 500 | 2,000 | Required on non-breathable bodies |
| 💧 Water Treatment Plant | +2% population growth rate | 400 | 500 | |
| 🧂 Desalination Plant | +1% population growth rate | 600 | 400 | Requires `desalination` tech |
| ♻️ Recycling Center | +2% mining efficiency; reduces waste | 300 | 1,000 | |

> **Scale note:** At 25M capacity per Housing Complex, Earth starts with ~335 complexes rather than 33,500.  Each new one you build adds a visible ~0.3% capacity boost.

---

### Industry

| Building | Effect | BP | Workers | Notes |
|----------|--------|----|---------|-------|
| ⚒ Mine | +15% mining efficiency | 400 | 5,000 | |
| 🏭 Refinery | +8% mining efficiency | 600 | 6,000 | Converts raw ore |
| 🏭 Factory | +10 BP/yr construction speed; −5% construction costs | 1,000 | 12,000 | Required for most BP output |
| ☁️ Atmospheric Processor | +0.75 Mt/yr atmospheric harvest | 600 | 3,000 | Gas-giant moons or dense atmospheres |
| ⚗️ Chemical Plant | +1 chemical processing unit/yr | 800 | 4,000 | Processes volatiles and polymers |
| 🛢️ Hydrocarbon Extractor | +10% mining efficiency | 1,200 | 2,500 | Oil/gas from crustal deposits |
| 🕳 Deep Drill | +25% deep mining efficiency | 2,000 | 10,000 | Requires `deep_drilling` tech |
| 🔦 Laser Drill | +50% deep mining efficiency | 6,000 | 4,000 | Requires `laser_drilling` tech |
| 🗻 Strip Mine | +100% bulk mining efficiency | 12,000 | 50,000 | Requires `strip_mining` tech |
| 💾 Semiconductor Fab | +8% research speed; +5% engineering speed | 5,000 | 5,000 | Requires `semiconductor_manufacturing` tech |
| 💊 Pharmaceutical Plant | +3% population growth rate | 800 | 4,000 | |

---

### Logistics

Logistics buildings increase the colony's **logistics capacity**.  Low capacity relative to demand causes an efficiency penalty on mines and research.

| Building | Effect | BP | Workers |
|----------|---------|----|---------|
| 🧲 Mass Driver | +5,000 logistics capacity | 2,000 | 2,500 |
| 🚡 Orbital Lift | +20,000 logistics capacity | 5,000 | 6,000 |
| 📦 Cargo Terminal | +2,000 logistics capacity | 300 | 3,000 |
| 🏗 Warehouse | +5% global stockpile capacity | 300 | 1,000 |

> **Tip:** Build Cargo Terminals early; they're cheap and prevent the mining efficiency penalty while you grow.

---

### Power

Buildings require power (MW/GW).  Power deficit reduces building output.  Build power plants before adding heavy industry.

| Building | Output | Fuel | BP | Workers |
|----------|--------|------|----|---------|
| ☀ Solar Power Plant | +5 GW | — | 200 | 500 |
| 💨 Wind Farm | +3 GW | — | 300 | 200 |
| 🌊 Hydroelectric Dam | +15 GW | — | 2,500 | 1,000 |
| 🌋 Geothermal Plant | +18 GW | — | 1,800 | 800 | Requires `geothermal_energy` tech |
| 🏭 Coal Power Plant | +10 GW | Coal | 800 | 2,000 |
| 🔥 Natural Gas Plant | +12 GW | Gas | 600 | 1,500 |
| ☢ Fission Reactor | +20 GW | Uranium | 1,500 | 4,000 |
| ⚡ Fusion Reactor | +40 GW | Deuterium | 5,000 | 8,000 | Requires `fusion_power` tech |

---

### Population & Food

Food is measured in **megatonnes per year (Mt/yr)**.  Per-capita consumption is **0.0001 Mt/person/yr** (100 t/person/yr).

| Building | Food output | Feeds | BP | Workers |
|----------|-------------|-------|----|---------|
| 🐄 Farm | 1,000 Mt/yr | ~10M people | 100 | 1,000 |
| 🌿 Greenhouse | 500 Mt/yr | ~5M people | 400 | 2,000 |
| 🐟 Aquaculture Facility | 750 Mt/yr | ~7.5M people | 500 | 1,500 |
| 🌾 Agricultural Dome | 4 Mt/yr | ~40K people (enclosed) | 600 | 4,000 |
| 🏥 Medical Center | +0.03% population growth rate per centre | 800 | 6,000 |

> **Example:** A new colony with 500K population needs at least 50 Mt/yr food.  That's 1 Farm, or 7 Greenhouses, or 1 Aquaculture Facility.

---

### Research

| Building | Effect | BP | Workers |
|----------|--------|----|---------|
| 🔬 Research Lab | +5% research speed | 1,000 | 8,000 |
| 🔩 Engineering Bay | +5% engineering speed | 1,200 | 10,000 |
| 🤖 AI Cluster | +15% research speed; +10% engineering speed | 4,000 | 2,000 | Requires `neural_networks` tech |
| 💾 Semiconductor Fab | +8% research speed; +5% engineering speed | 5,000 | 5,000 | See Industry |
| 🖥️ Data Center | +10% research speed; +8% engineering speed | 2,000 | 1,000 |

---

### Financial & Commerce

| Building | Effect | BP | Workers |
|----------|--------|----|---------|
| 🏪 Commercial Hub | +Credits income from trade | 500 | 8,000 |
| 🏦 Financial Center | +Credits income from banking | 1,500 | 10,000 |
| 🚢 Trade Port | +Credits income from import/export | 2,500 | 15,000 |

---

### Military & Shipbuilding

| Building | Effect | BP | Workers | Notes |
|----------|--------|----|---------|-------|
| ⚓ Shipyard | Enables ship construction; +10% ship efficiency; −10% build costs | 10,000 | 80,000 | Requires `orbital_construction` tech |
| 🚀 Missile Silo | Planetary anti-orbital defence | 3,000 | 5,000 | Requires `missile_systems` tech |
| 🛫 Launch Site | Surface-to-orbit access | 2,000 | 12,000 | |
| 🚀 Space Port | High-throughput orbital access | 4,000 | 20,000 | |
| 🛡️ Ground Defense Battery | Anti-orbital / anti-missile defence | 2,500 | 3,000 | |

---

## Building Scale & Design Philosophy

Building outputs are calibrated to **civilisation level**, not individual installations.  Each building represents a district-scale complex:

- **Housing Complex**: 25M residents — ~0.3% of Earth's 8.2B capacity per building
- **Farm**: 1,000 Mt/yr — feeds ~10M people; Earth starts with ~820 farms
- **Fission Reactor**: 20 GW — a major power plant (Earth has ~440 reactors representing ~8.8 TW)

This scale means that:

1. **Early colonies** are genuinely resource-limited — a 5M-person colony needs 1 Fission Reactor, 1 Farm, and 1 Housing Complex just to function.
2. **Each new building matters** — queuing a second Farm on a young colony doubles its food production.
3. **Earth is a reference point** — its hundreds of buildings are believable as planetary-scale infrastructure.

---

## Construction Workflow

1. Open the **Construction** tab.
2. Select the target colony from the colony dropdown (top left).
3. Browse buildings by category (collapsed/expanded headers).
4. Read the **effect lines** (green ▸ lines) on each card to understand what the building does.
5. Check resource costs (shown as `need / available`; green = OK, red = insufficient).
6. Adjust the **build multiplier** (×1 / ×5 / ×10) to queue batches.
7. Click **Queue** (or **Queue ×N**).
8. The construction queue at the bottom shows active projects with progress bars.

**Build Points (BP)** are produced by Factory buildings.  More Factories → faster construction across the whole colony.

**Workforce** must not exceed colony population × 0.4 (roughly 40% employment rate).  If workforce demand exceeds supply, new buildings will not operate at full efficiency.

---

## Population Growth

Population grows by ~0.9% per year at baseline.  Growth is multiplied by:

| Factor | Effect |
|--------|--------|
| Housing utilisation | Growth slows as housing fills; at 100% full → only 20% of normal growth |
| Food adequacy | Deficit → growth penalty; full food supply → 1.0× |
| Medical Centers | +0.03% per centre (up to +0.9% bonus) |
| Logistics efficiency | Penalty if logistics demand > capacity |

**To sustain 0.9% growth on Earth** (8.2B, ~74M net/yr) you need:

- ≥1% housing headroom (≥82M spare capacity → at least 4 spare Housing Complexes)
- Enough food: 8.2B × 0.0001 = 820,000 Mt/yr production
- Adequate logistics infrastructure

---

## Logistics Efficiency

Every non-logistics building generates **logistics demand** proportional to its workforce requirement.  Logistics buildings (Mass Drivers, Orbital Lifts, Cargo Terminals) provide **logistics capacity**.

```
Efficiency = capacity / demand   (clamped 0.0 – 1.0)
```

- If efficiency < 1.0, mining output and research output are **penalised**.
- Research has a minimum 50% output regardless of logistics.

**Rule of thumb for new outposts:** Build a Cargo Terminal alongside your first Mine.  It's cheap (300 BP) and avoids immediate efficiency penalties.

---

## Debug / Cheat Controls

Press **F12** while the Construction panel is open to toggle debug mode:

| Option | Effect |
|--------|--------|
| Free Construction | Build without resource costs |
| Instant Build | Skip the time-based construction queue |
| Bypass Tech | Show and build any building regardless of prerequisites |

These are useful for testing layouts or quickly getting a new colony up and running in a sandbox session.
