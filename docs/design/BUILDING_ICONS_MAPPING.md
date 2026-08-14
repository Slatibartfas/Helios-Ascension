# Building Icon Mapping — Helios Ascension

52-entry swap plan that retires the Unicode emoji in
`assets/data/buildings.ron` and replaces each one with the new
24×24 dark-on-white PNG in `assets/textures/ui/buildings/`. The user
applies the swap to `buildings.ron` once the icons are reviewed; the
table below is the reference for that edit.

**Do not run a search-and-replace from the `current emoji` column**
— many emojis are shared between buildings (e.g. `🏭` is used by
`Refinery` and `CoalPowerPlant`), and the table maps them by
**building id**, not by glyph.

## Visual concepts (subjects)

Each subject is the specific phrase that drove the
`image_synthesize` prompt for that icon. The user can re-prompt
any icon they want redone by replacing its subject.

## Mapping table

| # | id                          | display_name                | category       | current emoji | new icon filename              | subject (icon-artist prompt)                            | notes                          |
|---|-----------------------------|-----------------------------|----------------|---------------|--------------------------------|---------------------------------------------------------|--------------------------------|
| 1 | LifeSupport                 | Life Support                | Infrastructure | 🌬            | life-support.png               | fan or air processing unit with circular blades and a small air wave | shares emoji with `WindFarm`; differentiated by the fan blades |
| 2 | HabitatDome                 | Habitat Dome                | Infrastructure | 🏠            | habitat-dome.png               | large arcology habitat dome with a row of windows       | shares emoji with `Housing`    |
| 3 | Housing                     | Housing Complex             | Infrastructure | 🏙            | housing.png                    | three small apartment building silhouettes grouped together | the emoji 🏙 already implies a city cluster |
| 4 | UndergroundHabitat          | Underground Habitat         | Infrastructure | ⛏             | underground-habitat.png        | buried bunker showing half underground with a soil line and a small door on top | the emoji ⛏ is a pickaxe; the building is the *result* of digging, not the tool |
| 5 | Mine                        | Mine                        | Industry       | ⚒             | mine.png                       | surface mine headframe tower with pulley wheel and ground line | shares emoji with `EngineeringBay` |
| 6 | Refinery                    | Refinery                    | Industry       | 🏭            | refinery.png                   | factory with two smokestacks and a small smoke cloud    | shares emoji with `CoalPowerPlant` |
| 7 | Factory                     | Factory                     | Industry       | 🔧            | factory.png                    | factory building with a large gear in the foreground    | the emoji 🔧 is a wrench; the factory is the *place* that uses tools |
| 8 | DeepDrill                   | Deep Drill                  | Industry       | 🕳            | deep-drill.png                 | drill bit pointing down into a horizontal ground line   | the emoji 🕳 is "hole"; the drill is what makes the hole |
| 9 | LaserDrill                  | Laser Drill                 | Industry       | 🔦            | laser-drill.png                | drill tower with a downward beam of light               | shares emoji with `Flashlight` (rarely) |
| 10 | StripMine                  | Strip Mine                  | Industry       | 🗻             | strip-mine.png                 | stepped terraced mountain with three flat levels        | the emoji 🗻 is a generic mountain; the strip mine is the *terraced* version |
| 11 | AtmosphericProcessor       | Atmospheric Processor        | Industry       | ☁             | atmospheric-processor.png      | vertical cylindrical tank with a small cloud above it   | a tank that processes air; the cloud is the input |
| 12 | ChemicalPlant               | Chemical Plant              | Industry       | ⚗             | chemical-plant.png             | chemistry Erlenmeyer flask with three small bubbles     | shares emoji with `Alembic` |
| 13 | HydrocarbonExtractor        | Hydrocarbon Extractor       | Industry       | ⛽            | hydrocarbon-extractor.png      | oil derrick with a pump and small oil drop              | shares emoji with `FuelPump` |
| 14 | MassDriver                  | Mass Driver                 | Logistics      | 🧲            | mass-driver.png                | horseshoe magnet shape with a small projectile on a rail | the emoji 🧲 is a magnet; the mass driver is the electromagnetic launcher |
| 15 | OrbitalLift                 | Orbital Lift                | Logistics      | 🚡            | orbital-lift.png               | very tall thin tower with a cable connecting to a small orbital station | the emoji 🚡 is "aerial tramway"; the orbital lift is the *space* version |
| 16 | CargoTerminal               | Cargo Terminal              | Logistics      | 📦            | cargo-terminal.png             | cube-shaped package box with sealing tape across the top |                                |
| 17 | SolarPower                  | Solar Power Plant           | Power          | ☀             | solar-power.png                | tilted rectangular panel grid with a small sun in the upper-left corner | shares emoji with `DHe3FusionReactor`; the panel grid is the differentiator |
| 18 | FissionReactor              | Fission Reactor             | Power          | ☢             | fission-reactor.png            | nuclear cooling tower with a radiation trefoil symbol   | shares emoji with `BreederReactor`; the trefoil is the differentiator |
| 19 | FusionReactor               | Fusion Reactor              | Power          | ⚡             | fusion-reactor.png             | toroidal fusion reactor ring (donut shape viewed from above) |                                |
| 20 | DTFusionReactor             | D-T Fusion Reactor          | Power          | ⚛             | dt-fusion-reactor.png          | atom symbol with a central nucleus and three elliptical electron orbits |                                |
| 21 | DHe3FusionReactor           | D-He3 Fusion Reactor        | Power          | ☀             | dhe3-fusion-reactor.png        | fusion reactor with a sun inside a magnetic ring        | shares emoji with `SolarPower`; the magnetic ring is the differentiator |
| 22 | ThoriumReactor              | Thorium Reactor             | Power          | ♨             | thorium-reactor.png            | reactor vessel with rising heat waves or steam lines    |                                |
| 23 | BreederReactor              | Breeder Reactor             | Power          | ☢             | breeder-reactor.png            | nuclear cooling tower with vertical fuel rods inside it | shares emoji with `FissionReactor`; the visible fuel rods are the differentiator |
| 24 | AgriDome                    | Agricultural Dome           | Population     | 🥬            | agri-dome.png                  | geodesic dome with a small leaf inside                  |                                |
| 25 | Farm                        | Farm                        | Population     | 🌾            | farm.png                       | single wheat stalk with grain head and two leaves      | shares emoji with `Greenhouse`'s plant theme |
| 26 | MedicalCenter               | Medical Center              | Population     | 🏥            | medical-center.png             | hospital building with a cross symbol on the front      |                                |
| 27 | ResearchLab                 | Research Lab                | Research       | 🔬            | research-lab.png               | simple microscope with eyepiece, arm, stage, and base   |                                |
| 28 | EngineeringBay              | Engineering Bay             | Research       | 🔩            | engineering-bay.png            | single large bolt with a gear behind it                 |                                |
| 29 | AiCluster                   | AI Cluster                  | Research       | 🤖            | ai-cluster.png                 | microchip with three neural network nodes connected by lines | the emoji 🤖 is a robot; the cluster is a *rack* of AI compute |
| 30 | CommercialHub               | Commercial Hub              | Financial      | 🏪            | commercial-hub.png             | storefront with an awning and a small shop sign         |                                |
| 31 | FinancialCenter             | Financial Center            | Financial      | 🏦            | financial-center.png           | bank building with four classical columns and a triangular pediment |                                |
| 32 | TradePort                   | Trade Port                  | Financial      | 🚢            | trade-port.png                 | cargo ship with stacked containers on the deck          |                                |
| 33 | Shipyard                    | Shipyard                    | Military       | ⚓             | shipyard.png                   | ring-shaped orbital shipyard with a ship inside it      | the emoji ⚓ is a sea anchor; the shipyard is *orbital* |
| 34 | MissileSilo                 | Missile Silo                | Military       | 🚀            | missile-silo.png               | missile pointing up inside a ground silo with hatch lines | shares emoji with `SpacePort` and `LaunchSite`; the underground hatch is the differentiator |
| 35 | LaunchSite                  | Launch Site                 | Military       | 🛫            | launch-site.png                | rocket on a launch pad with a service tower beside it   | the emoji 🛫 is an airplane departure; the launch site is a *rocket* pad |
| 36 | WindFarm                    | Wind Farm                   | Power          | 💨            | wind-farm.png                  | single wind turbine with three blades and a tall tower  | shares emoji with `LifeSupport`; the turbine tower is the differentiator |
| 37 | HydroelectricDam            | Hydroelectric Dam           | Power          | 🌊            | hydroelectric-dam.png          | hydroelectric dam wall with horizontal water lines below |                                |
| 38 | GeothermalPlant             | Geothermal Plant            | Power          | 🌋            | geothermal-plant.png           | volcano cone with rising steam waves from the top       |                                |
| 39 | CoalPowerPlant              | Coal Power Sector           | Power          | 🏭            | coal-power-plant.png           | power plant with a tall smokestack and smoke cloud       | shares emoji with `Refinery`; the tall smokestack is the differentiator |
| 40 | NaturalGasPlant             | Gas Power Sector            | Power          | 🔥            | natural-gas-plant.png          | gas turbine with a flame coming out of one end          |                                |
| 41 | SemiconductorFab            | Electronics Industry        | Industry       | 💾            | semiconductor-fab.png          | square microchip with small pins on all four sides      | the emoji 💾 is a floppy disk; the fab is a *modern* chip plant |
| 42 | PharmaceuticalPlant         | Pharmaceutical Sector       | Industry       | 💊            | pharmaceutical-plant.png       | medical capsule pill, half dark half light, horizontal  |                                |
| 43 | WaterTreatmentPlant         | Water Management Complex    | Infrastructure | 💧            | water-treatment-plant.png      | water treatment tank with a single water drop on the side |                                |
| 44 | DesalinationPlant           | Desalination Complex        | Infrastructure | 🧂            | desalination-plant.png         | wave with a small salt crystal hexagon on top           | the emoji 🧂 is just salt; the wave is the water being processed |
| 45 | RecyclingCenter             | Industrial Recycling Complex| Infrastructure | ♻️            | recycling-center.png           | three curved recycling arrows in a triangle loop        |                                |
| 46 | Greenhouse                  | Greenhouse Complex          | Population     | 🌿            | greenhouse.png                 | small greenhouse with a triangular roof and plants inside | shares emoji with `Farm`'s plant theme; the greenhouse enclosure is the differentiator |
| 47 | AquacultureFacility         | Aquaculture Complex         | Population     | 🐟            | aquaculture-facility.png       | fish inside a square tank                               |                                |
| 48 | DataCenter                  | Computation Hub             | Research       | 🖥️            | data-center.png                | server rack with three stacked horizontal rectangles    |                                |
| 49 | SpacePort                   | Space Port                  | Military       | 🚀            | space-port.png                 | launch complex with three rocket pads in a row          | shares emoji with `MissileSilo` and `LaunchSite`; the multi-pad row is the differentiator |
| 50 | GroundDefenseBattery        | Ground Defense Battery      | Military       | 🛡️            | ground-defense-battery.png     | defense turret with a shield in front of it             |                                |
| 51 | Warehouse                   | Resource Depot              | Logistics      | 🏗             | warehouse.png                  | warehouse building with a sloped roof and a large loading door |                                |
| 52 | OrbitalSurveyStation        | Orbital Survey Station      | Research       | 🛰            | orbital-survey-station.png     | satellite with a central body and two solar panel wings |                                |

## Emoji glyph retirement summary

The 52 buildings use 48 distinct Unicode emoji glyphs. The table
above retires all 48 in favour of the new PNGs. Glyphs that map to
multiple buildings (and were the original source of glyph
ambiguity):

- `🏭` → `Refinery` (smokestack factory), `CoalPowerPlant` (tall single smokestack)
- `🏠` → `HabitatDome` (arcology), `Housing` (apartment cluster)
- `⛽` → `HydrocarbonExtractor` (oil derrick) — single use, listed for completeness
- `🏗` → `Warehouse` (sloped-roof warehouse) — single use, listed for completeness
- `🛡️` → `GroundDefenseBattery` (turret + shield) — single use
- `🚀` → `MissileSilo` (underground), `LaunchSite` (pad), `SpacePort` (multi-pad)
- `🛰` → `OrbitalSurveyStation` (satellite) — single use
- `🏥` → `MedicalCenter` (hospital cross) — single use
- `🏦` → `FinancialCenter` (bank columns) — single use
- `🏪` → `CommercialHub` (storefront awning) — single use
- `☀` → `SolarPower` (panel grid), `DHe3FusionReactor` (sun in magnetic ring)
- `☢` → `FissionReactor` (cooling tower + trefoil), `BreederReactor` (cooling tower + fuel rods)
- `🌾` → `Farm` (single stalk) — primary, but visually similar to the `Greenhouse` herb 🌿
- `🌿` → `Greenhouse` (triangular greenhouse) — primary, but visually similar to `Farm` 🌾
- `🌬` → `LifeSupport` (fan blades) — primary, but visually similar to `WindFarm` 💨
- `💨` → `WindFarm` (turbine) — primary, but visually similar to `LifeSupport` 🌬
- `🏙` → `Housing` (apartment cluster) — primary, distinct from `🏠`
- `⛏` → `UndergroundHabitat` (bunker) — primary, repurposed
- `⚒` → `Mine` (headframe) — primary, repurposed
- `🔧` → `Factory` (factory + gear) — primary, repurposed
- `🕳` → `DeepDrill` (drill bit + ground) — primary, repurposed
- `🔦` → `LaserDrill` (drill + light beam) — primary, repurposed
- `🗻` → `StripMine` (terraced mountain) — primary, repurposed
- `☁` → `AtmosphericProcessor` (tank + cloud) — primary, repurposed
- `⚗` → `ChemicalPlant` (flask + bubbles) — primary, repurposed
- `🧲` → `MassDriver` (magnet + projectile) — primary, repurposed
- `🚡` → `OrbitalLift` (tower + cable + station) — primary, repurposed
- `📦` → `CargoTerminal` (box + tape) — primary
- `⚡` → `FusionReactor` (toroid) — primary
- `⚛` → `DTFusionReactor` (atom) — primary
- `♨` → `ThoriumReactor` (heat waves) — primary
- `🥬` → `AgriDome` (geodesic dome + leaf) — primary
- `🔬` → `ResearchLab` (microscope) — primary
- `🔩` → `EngineeringBay` (bolt + gear) — primary
- `🤖` → `AiCluster` (chip + neural network) — primary
- `🚢` → `TradePort` (cargo ship) — primary
- `⚓` → `Shipyard` (orbital ring + ship) — primary
- `🛫` → `LaunchSite` (rocket + tower) — primary
- `🌊` → `HydroelectricDam` (dam wall) — primary
- `🌋` → `GeothermalPlant` (volcano + steam) — primary
- `🔥` → `NaturalGasPlant` (gas turbine + flame) — primary
- `💾` → `SemiconductorFab` (chip) — primary
- `💊` → `PharmaceuticalPlant` (capsule) — primary
- `💧` → `WaterTreatmentPlant` (tank + drop) — primary
- `🧂` → `DesalinationPlant` (wave + salt) — primary
- `♻️` → `RecyclingCenter` (3 arrows) — primary
- `🐟` → `AquacultureFacility` (fish + tank) — primary
- `🖥️` → `DataCenter` (server rack) — primary

## Applying the swap

Once the user has reviewed the icons in `assets/textures/ui/buildings/`
(plus the 24×24 previews in `_resize/`), the swap is a per-building
edit of `buildings.ron`:

1. For each row in the table above, replace the `icon: "<emoji>"`
   line with `icon: "textures/ui/buildings/<filename>.png"`.
2. The RON schema is already wired for this — `icon: String` is
   already a field on every building entry.
3. The legacy egui panel (`src/ui/construction_panel.rs`, deleted in v0.5.2 — see `src/ui/construction/` directory) currently
   renders the emoji directly via `egui::RichText`. **It will break
   on PNG paths** — that panel needs to be updated to render
   `Handle<Image>` (or read the RON `icon:` field and look it up
   via the new `BuildingIcons` resource). Treat that as a
   follow-up against the egui stack.
4. The bevy_ui canary does not yet load building icons; see the
   "Load contract" section of `assets/textures/ui/buildings/README.md`
   for the additive work needed before it can render these.

## Buildings where the icon subject is a stretch

A few buildings had emoji mappings that were technically wrong
(emojis chosen for visual association, not for accuracy). The new
icons deliberately correct this where the building's purpose is
clear from the description:

- `UndergroundHabitat` (⛏) — the emoji is a pickaxe; the building
  is a buried habitat. New icon is a bunker.
- `HydrocarbonExtractor` (⛽) — the emoji is a fuel pump; the
  building is an oil/gas extractor. New icon is a derrick.
- `MissileSilo` (🚀) — the emoji is a rocket; the building is a
  *silo* (mostly underground). New icon is a missile in a hatch.
- `Shipyard` (⚓) — the emoji is a sea anchor; the building is an
  *orbital* shipyard. New icon is a ring with a ship.
- `OrbitalLift` (🚡) — the emoji is an aerial tramway; the
  building is a *space* elevator. New icon is a tower with a
  cable to orbit.
- `MassDriver` (🧲) — the emoji is a magnet; the building is an
  electromagnetic launcher. New icon is a magnet with a
  projectile on a rail.
- `AiCluster` (🤖) — the emoji is a humanoid robot; the building
  is a *rack* of AI compute. New icon is a chip with neural
  network nodes.
- `HydroelectricDam` (🌊) — the emoji is a wave; the building is
  a dam. New icon is a dam wall.
- `DataCenter` (🖥️) — the emoji is a desktop monitor; the
  building is a server rack. New icon is a stacked rack.
- `Factory` (🔧) — the emoji is a wrench; the building is a
  factory. New icon is a factory with a gear.
- `StripMine` (🗻) — the emoji is a generic mountain; the
  building is a *terraced* strip mine. New icon is terraced levels.

These are documented here so the user can re-prompt any of them if
they prefer a different interpretation.
