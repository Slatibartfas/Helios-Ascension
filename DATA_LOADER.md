# RON Data Loader (DELA-3)

**Author:** CTO
**Date:** 2026-06-04
**Status:** v1 — initial implementation
**Supersedes:** ad-hoc `Startup` loaders in `research`, `colony`
**Implements:** `ARCHITECTURE_BASELINE.md` §3 "RON Data Pipeline"

---

## 1. Purpose

`DataLoaderPlugin` is the **first ECS system** built on top of the Bevy 0.18
architecture baseline, and the single canonical entry point for loading RON
data at startup. It is the pattern every future loader
(`ship_modules.ron`, `ship_hulls.ron`, logistics networks, multi-star
systems, …) must follow.

The plugin is the observable surface of the data pipeline: a smoke check
runs in `PostStartup` and logs every file's status; downstream systems,
UI overlays, and modding tools read the `LoadedDataManifest` resource
rather than reaching into per-file resources.

---

## 2. Pipeline

```text
assets/data/*.ron
  -> DataLoaderPlugin::scan_and_load_data_files (Startup)
       1. read_dir("assets/data"), filter *.ron, sort alphabetically
       2. for each file:
            a. read_to_string
            b. parse as `ron::Value` (generic syntax check)
            c. classify (LoaderOwned | OwnedByOtherPlugin | AwaitingLoader)
            d. record LoadedDataEntry
       3. for each LoaderOwned file:
            a. dispatch typed loader (load_technologies, load_buildings, …)
            b. typed loader issues `commands.insert_resource(TechData)` etc.
  -> DataLoaderPlugin::log_loaded_data_smoke_check (PostStartup)
       - queries `LoadedDataManifest` and one typed resource
       - logs one summary line per file
```

The architecture baseline's invariant
(`assets/data/*.ron -> ron::de::from_reader() -> App::resources`) is
preserved: this plugin is the only place that reads `assets/data/*.ron`,
and every typed `Resource` is inserted into the `World` before the first
`Update` tick.

---

## 3. Load strategy chosen

**Manual `ron::from_str` + `fs::read_to_string` at startup.**
*Not* `bevy::asset::AssetServer`.

### Rationale

| Option | Pros | Cons |
|--------|------|------|
| `AssetServer` (handles) | Async, hot-reloadable, consistent with audio/textures | Data is not available in `Startup`; needs polling or `OnAdd` observers; silent on parse errors |
| `ron::from_str` + `include_str!` | Compile-time embedded, no runtime IO | Forces a rebuild on every data edit; doubles binary size for large files |
| **`ron::from_str` + `fs::read_to_string`** *(chosen)* | Synchronous, hard parse errors at startup, available in `Startup` as `Resource`s, no double-buffering | No hot-reload; data must be on disk relative to CWD |

We pick the chosen option because:

1. **The data is content, not asset state.** RON files in `assets/data/`
   are hand-edited modder-facing content, not streaming assets. The
   developer iteration loop already requires a rebuild for Rust changes;
   a data edit with `include_str!` adds a build cycle for no benefit.
2. **`Startup`-schedule availability.** Game logic in the first frame
   queries `Res<TechnologiesData>` etc. With `AssetServer` the handle is
   unresolved at that point, forcing a two-phase system or polling.
   With `fs::read_to_string` the `Resource` is present the moment the
   `Startup` schedule returns.
3. **Hard parse errors.** A bad RON file currently produces a Bevy
   `error!` log line and a default `Resource` (so the game still boots).
   `AssetServer` would log the error and silently serve a missing handle.
4. **Convention match.** The pre-existing per-module loaders
   (`research::data::load_technologies`, `colony::data::load_buildings`)
   already use this exact pattern. The plugin reuses those functions
   via `IntoSystem`, so we don't duplicate the parsing logic.

---

## 4. File classification

Every `*.ron` file under `assets/data/` is classified into one of three
buckets by the `classify()` function in `src/data_loader/mod.rs`.

### `DataFileKind::LoaderOwned`

The plugin parses and inserts a typed `Resource`.

| File | Typed `Resource` | Parser function |
|------|------------------|-----------------|
| `technologies.ron` | `TechnologiesData` (in `research`) | `research::data::load_technologies` |
| `buildings.ron` | `BuildingsData` (in `colony`) | `colony::data::load_buildings` |

### `DataFileKind::OwnedByOtherPlugin`

The plugin runs the generic syntax check (`ron::Value` parse) but does
**not** insert a `Resource`. The owning plugin loads the file in its own
`Startup` system. Doubling the insert would risk a race between two
parsers reading the same bytes.

| File | Owning plugin |
|------|---------------|
| `solar_system.ron` | `plugins::solar_system::SolarSystemPlugin` |
| `planet_textures.ron` | `plugins::starmap::StarmapPlugin` |

### `DataFileKind::AwaitingLoader`

The file is named in the architecture baseline or expected by a future
LGD-scope feature, but no parser is wired yet. The plugin still records
its presence in the manifest. If the file is missing, the loader logs a
`Missing` warning; if present, the syntax check passes and the entry is
marked `Loaded` even though no typed `Resource` was inserted.

| File | Reason |
|------|--------|
| `ship_modules.ron` | LGD scope (post-DELA-3) |
| `ship_hulls.ron` | LGD scope (post-DELA-3) |
| anything else under `assets/data/` | modder-added; warns but does not fail |

---

## 5. Resource surface

```rust
#[derive(Resource, Debug, Clone, Default)]
pub struct LoadedDataManifest {
    pub entries: Vec<LoadedDataEntry>,  // sorted, one per .ron file
}

pub struct LoadedDataEntry {
    pub path: String,                  // e.g. "assets/data/buildings.ron"
    pub size_bytes: Option<u64>,
    pub kind: DataFileKind,
    pub status: LoadStatus,            // Loaded | Missing | ParseError | ReadError | DelegatedToOtherPlugin
    pub error: Option<String>,
}
```

The manifest is registered as a `Resource` by `DataLoaderPlugin::build`.
Any system can `Res<LoadedDataManifest>` and inspect what loaded. The
shipment of the canonical typed resources (`TechnologiesData`,
`BuildingsData`, etc.) is unchanged from before — this plugin does not
introduce a new type for the existing data.

---

## 6. Deviations from the architecture baseline

**None.** The plugin implements the §3 RON Data Pipeline exactly:

* `assets/data/*.ron` is the source of truth.
* `ron::de::from_reader` (via `ron::from_str`) is the deserializer.
* Data is inserted into `App::resources` (via `commands.insert_resource`).
* Systems query with `Res<T>`.

The §1 ECS-schedule invariant (no game logic in `apply_deferred`) and
the §2 `EguiPrimaryContextPass` invariant are unaffected. The plugin
adds a `Startup` system and a `PostStartup` system; both are inside the
canonical `enter -> run -> exit` window.

**One behavioral change vs. pre-DELA-3:** the duplicate `Startup`
registrations of `load_technologies` (in `ResearchPlugin`) and
`load_buildings` (in `ColonyPlugin`) are removed. Those functions are
still `pub` in their modules for unit tests, but they are no longer
scheduled as independent startup systems. `DataLoaderPlugin` is now
the single scheduling point and dispatches them via `IntoSystem` during
its own startup pass.

---

## 7. Adding a new RON file (recipe for LGD)

1. Place the file under `assets/data/`.
2. Define the typed `Resource` in the owning module
   (`src/<domain>/types.rs` or similar).
3. Write a `load_<thing>(mut commands: Commands)` function in
   `<domain>::data`, mirroring the existing `load_technologies` /
   `load_buildings` pattern.
4. Add a `DataFileKind::LoaderOwned` arm to `classify()` in
   `src/data_loader/mod.rs` that matches the new filename.
5. Add a `dispatch_known_loader` arm that calls the new function.
6. Update the table in §4 above.
7. Add at least one integration test under `tests/`.

If the file is owned by another plugin (i.e., a different plugin already
parses it), use `OwnedByOtherPlugin` instead — the plugin will only
syntax-check it. Do **not** add it as `LoaderOwned` if some other plugin
also loads it; the manifest will report a `ParseError` if the schemas
disagree and that surfaces the conflict.

---

## 8. CI expectations

* `cargo build --release --locked` (existing `build-and-test` job) must
  pass.
* `cargo test --locked` must pass. The plugin ships with three unit
  tests in `src/data_loader/mod.rs`:
  * `classify_known_files` — file classification
  * `manifest_counts_track_entries` — manifest counters
  * `all_present_ron_files_parse_as_value` — generic syntax check on
    every `*.ron` in `assets/data/`
* No new CI dependencies. The plugin uses `ron = "0.8"` and
  `serde = "1.0"`, both already in `Cargo.toml`.

The host does **not** run `cargo build` or `cargo test`; CI is the
oracle (per DELA-3 instructions).
