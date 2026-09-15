# Large astronomy data dumps — removed after extraction

The three CSVs that used to live here were **removed from the repo on
2026-09-15**. Their data has been extracted into the gameplay sidecars
that the game actually reads (`assets/data/solar_system.ron` and
`assets/data/asteroids.ron`), so the raw dumps were no longer needed.

**None of these files were ever read at runtime.** The extraction is
complete and the game does not depend on them.

| File | Size (was) | Source if you need it again |
|---|---|---|
| `JPL_SmallBodiesList.csv` | ~83 MB | <https://ssd.jpl.nasa.gov/tools/sbdb_lookup.html#/?csv=true> |
| `Exoplanets_NASA.csv` | ~73 MB | <https://exoplanetarchive.ipac.caltech.edu/TAP/sync?query=SELECT+*+FROM+ps&format=csv> |
| `JPL_CometsList.csv` | ~40 KB | <https://ssd.jpl.nasa.gov/tools/sbdb_lookup.html#/?csv=true&sb-cdata=ac> |

## Why they are gone

- **Repo bloat.** The three CSVs added ~157 MB to every clone and
  `git fetch`, and none of them compressed in the pack (CSV/PNG store
  near-raw), so they cost that much permanently in `.git` history too.
- **Not loaded.** `git grep` over `src/`, `tests/`, `build.rs`, and
  `Cargo.toml` returned zero references to any of them. The `csv` crate
  is not a dependency.
- **Extracted.** The JPL small-body data drives `assets/data/asteroids.ron`
  (450 asteroid entries joined by body name), and the comet data is
  inline in `assets/data/solar_system.ron` (50 `body_type: Comet` entries).
- **Exoplanets remain aspirational.** `src/astronomy/exoplanets.rs`
  defines the `ConfirmedPlanet` data model and `RealPlanet` marker, but
  the NASA Exoplanet Archive loader is still deferred to the v0.6
  interstellar milestone. If that work resumes, re-fetch the CSV with the
  command below rather than resurrecting it from history.

## If you need them locally again

```powershell
# JPL Small-Body Database (CSV, all known small bodies)
Invoke-WebRequest -Uri "https://ssd.jpl.nasa.gov/api/sbdb_query.csv?fields=..." `
    -OutFile "assets\data\JPL_SmallBodiesList.csv"

# NASA Exoplanet Archive (TAP service, full Planetary Systems table)
Invoke-WebRequest -Uri "https://exoplanetarchive.ipac.caltech.edu/TAP/sync?query=SELECT+*+FROM+ps&format=csv" `
    -OutFile "assets\data\Exoplanets_NASA.csv"

# JPL Comets subset
Invoke-WebRequest -Uri "https://ssd.jpl.nasa.gov/api/sbdb_query.csv?sb-cdata=ac&fields=..." `
    -OutFile "assets\data\JPL_CometsList.csv"
```

The top-level `.gitignore` still lists these paths, so a locally fetched
copy stays untracked and will not be re-committed by accident.

## See also

- `docs/design/DATABASE_INTEGRATION.md` — design notes for the (planned)
  ingestion pipeline.
- `assets/data/EXOPLANETS_IMPLEMENTATION.md` — design sketch for adding
  confirmed exoplanets to `nearest_stars_raw.json`.
