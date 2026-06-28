# Large astronomy data dumps — local only

The CSV files listed below are **not tracked in git** (see top-level
`.gitignore`). They are large reference dumps that may be useful for
offline analysis or future data ingestion, but **none of them are read by
the game at runtime**. Clone the repo and download only what you need.

| File | Size | Source |
|---|---|---|
| `JPL_SmallBodiesList.csv` | ~83 MB | <https://ssd.jpl.nasa.gov/tools/sbdb_lookup.html#/?csv=true> |
| `Exoplanets_NASA.csv` | ~73 MB | <https://exoplanetarchive.ipac.caltech.edu/TAP/sync?query=SELECT+*+FROM+ps&format=csv> |
| `JPL_CometsList.csv` | ~40 KB | <https://ssd.jpl.nasa.gov/tools/sbdb_lookup.html#/?csv=true&sb-cdata=ac> |

## Why they are not in the repo

- **Repo bloat.** Three CSVs add ~157 MB to every clone and `git fetch`.
- **Not loaded.** `git grep` over `src/`, `tests/`, and `Cargo.toml`
  returns zero references to any of these files. The `csv` crate is not
  even a dependency.
- **Aspirational architecture.** Several docs (`ARCHITECTURE.md`,
  `CLAUDE.md`, `README.md`) describe `src/astronomy/exoplanets.rs` as
  ingesting the NASA Exoplanet Archive. The struct definitions and tests
  are in place, but the loader is not — it is intentionally deferred
  until the v0.6 interstellar travel milestone needs real exoplanet
  targets.
- **JPL Small Bodies / Comets** were added in commit `b0c3aa5` but the
  planned `AsteroidLoader` system was never wired up. The procedurally
  generated main belt / Trojans / Kuiper belt bodies in
  `src/astronomy/procedural.rs` carry the gameplay; the CSV was
  intended as a future realism upgrade.

## If you need them locally

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

## See also

- `docs/design/DATABASE_INTEGRATION.md` — design notes for the (planned)
  ingestion pipeline.
- `assets/data/EXOPLANETS_IMPLEMENTATION.md` — design sketch for adding
  confirmed exoplanets to `nearest_stars_raw.json`.
