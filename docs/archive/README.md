# docs/archive — Historical Documents

This folder holds documents that were current **during early development milestones** (v0.1 → v0.2) and have since been superseded. They are kept on disk so git history is not disrupted and so contributors can trace the evolution of design decisions, but **the live, current-state documentation lives in**:

- `docs/QUICKSTART.md` — first-time player walkthrough
- `docs/COLONIES.md` — colony management & building reference
- `docs/SURVEY.md` — survey system player manual (v0.5.0)
- `docs/SHIPBUILDING.md` — shipbuilding & freighter templates
- `docs/RESOURCES.md` — resource catalogue
- `docs/RESEARCH_MODDING.md` — tech-tree modding guide
- `docs/MODDING.md` — textures, bodies, buildings, ships modding
- `docs/UI.md` — UI panels, theme tokens, layout patterns
- `docs/UI_LAYOUT_PATTERNS.md` — UI layout primitives reference
- `docs/TESTING.md` — testing conventions
- `docs/ASTRONOMY.md` — orbital mechanics reference
- `docs/design/` — design specifications (LOGISTICS_NETWORK, SURVEY_REWORK, …)
- `docs/tech_tree_pacing.md` — research pacing notes

If a file in this archive contradicts a current doc, **the current doc wins**. The archive files are a snapshot of what the codebase looked like at the time of writing and may reference modules, components, or counts that have since been renamed, removed, or re-counted.

---

## Files

| File | Era | Superseded by |
|------|-----|---------------|
| `EXPANSION_SUMMARY.md` | v0.1.x → v0.2 | `README.md` (Project Structure, Architecture) |
| `FEATURES_IMPLEMENTED.md` | v0.2.0 | `README.md` (Features), `ARCHITECTURE.md` |
| `IMPLEMENTATION_COMPLETE.md` | v0.2.0 | `README.md`, `ARCHITECTURE.md` |
| `IMPLEMENTATION_SUMMARY.md` | v0.2.0 | `ARCHITECTURE.md` |
| `PERFORMANCE_IMPROVEMENTS.md` | v0.2.x | (no live doc; reference for performance-sensitive changes) |
| `UPGRADE_SUMMARY.md` | v0.2.x → v0.3 | `ROADMAP.md` |
| `QUESTIONS_ANSWERED.md` | v0.2 | (historical Q&A; some answers still apply, some don't) |
| `SCIENTIFIC_SOURCES.md` | v0.2 | `docs/ASTRONOMY.md` |
| `TEXTURE_IMPLEMENTATION.md` | v0.2.x | `docs/MODDING.md` (Textures & Celestial Bodies) |
| `UI_CHANGES.md` | v0.2.x → v0.3 | `docs/UI.md` |
| `STARMAP_IMPLEMENTATION.md` | v0.3 | `ARCHITECTURE.md` (StarmapPlugin), `ROADMAP.md` |
| `CODE_REVIEW_FIXES.md` | v0.3 | (historical review log) |

---

## Why keep them?

These files capture **why** certain design decisions were made (e.g. why `SpaceCoordinates` uses `DVec3` instead of `Vec3`, why the 3-tier `SurveyLevel` was replaced by the 8-dimension model, why the per-body stockpile was a v0.4 redesign rather than a v0.3 patch). The git log records the *what* but not always the *why*; these docs preserve the reasoning for future contributors and reviewers.

For the current state of the codebase, **always start with the live docs above**.