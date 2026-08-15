# Splash / First-Frame Stall Prevention (Helios-Ascension)

**Status:** Regression fixed 2026-08-05 (commit `2d3223d` on `rework-ui-design`).
**Root cause:** git-bisected, worktree-per-commit, to commit `4d4dc23`
(`feat(ui): dedicated energy icon for power breakdown`).

## The symptom

The splash window stays black/frozen for ~20 s on startup. Then the splash
appears, lives for ~3 s, and the menu takes over. **The player never
sees the logo render during the stall** — only the *post-stall* splash
lifetime is what they perceive.

This is a single-frame stall on frame 0 of the app — `Time<Real>` records
the whole stall as the frame's `delta`, so systems that try to "use up
real time" (e.g. splash dismiss timers) trip their fallback immediately
on the first painted frame.

## Why this can reappear

The stall is **purely a CPU cost on the main thread** — not a GPU stall,
not a plugin ordering bug, not a network call. Anything that:

1. Loads asynchronously (PNG via `AssetServer`),
2. Becomes available all at once at some frame N,
3. Has per-item work that is O(pixels · items) — e.g. per-pixel loops,
   Lanczos resampling, RGBA→RGBA transforms —
4. Runs in a `Update` system that processes the whole batch in one tick

…will reproduce the same stall. The pattern is common in UI/icon
initialisation, atlas baking, and shader warm-up.

## What the fix looks like

For `src/ui/resource_icons.rs` (the original culprit):

```rust
// load_resource_icons
const MAX_ICONS_PER_FRAME: usize = 2;
let mut processed_this_frame = 0usize;
for &resource in ResourceType::all() {
    if processed_this_frame >= MAX_ICONS_PER_FRAME { break; }
    // ... load + process ...
    processed_this_frame += 1;
}
```

And in `post_process_resource_icons`, process **at most one** pre-baked
icon per frame (the post-process runs every frame after icons are
queued).

For `src/ui/launch/splash.rs` (the splash timer fallback):

```rust
pub const MAX_SPLASH_FRAME_DT_S: f32 = 0.25;  // clamp per-frame dt
let raw_dt = real_time.delta_secs();
let dt = raw_dt.min(MAX_SPLASH_FRAME_DT_S);
```

Without the clamp, a multi-second first-frame stall makes the splash
timer think it has already served its full lifetime and dismisses
before the logo has been on screen for even a single frame.

## The bisection method (reusable for any frame-stall investigation)

When a "game hangs for N seconds at startup" or "menu takes N seconds
to appear" report comes in, **do not** try to fix it by reasoning about
plugins, schedules, or renderer order. Bisect by worktree-per-commit:

```bash
# 1. Find a baseline reference (e.g. main b607529)
git worktree add ../bisect-good <good-commit>      # known fast
git worktree add ../bisect-bad <bad-commit>       # current HEAD

# 2. In each worktree, add a probe to src/ui/launch/splash.rs that
#    logs the wall-clock time to first PostUpdate frame:
#
#    bevy::log::info!(
#       "BISECT_POST frame={} t={:.3}s",
#       frame_count.0,
#       startup_time.elapsed_secs()
#    );

# 3. In each worktree:
cargo build --profile fast
./target/fast/helios_ascension.exe 2>&1 | grep BISECT_POST
# Difference in t= tells you how long frame 0 took to paint.

# 4. Walk the commit graph between good and bad, one worktree per
#    commit, log the t= value for each. The single commit where t=
#    jumps from <1 s to >10 s is the regression source.

# 5. git worktree remove --force ../bisect-* each time
```

**Why worktree-per-commit, not `git bisect run`:** the project is
shared across multiple agents. `git bisect run cargo` repeatedly
checks out branches and rebuilds `target/`, which trashes the
incremental build cache other agents depend on. Fresh worktrees
keep `target/` scoped to the probe.

**Why not `git stash` to "isolate the bug":** see the
"Multi-Agent Worktree Safety" section in `.github/copilot-instructions.md`.
Stashing in a shared worktree has destroyed other agents' in-flight
work multiple times. Never `git stash` here.

## What to look for when reviewing a PR that touches icon/atlas/asset loading

- **Async + batch process** is the dangerous pattern. If a system
  processes an unknown or large number of items in one `Update` tick,
  and each item's cost is O(pixels) or O(vertices), demand a per-frame
  budget.
- **Per-pixel RGBA loops** on 1024×1024 buffers: 4.2M pixel writes
  × ~38 items = ~160M pixel writes = measurable seconds even on
  modern CPUs. If a transform is "free" because the output buffer is
  the same shape, question whether it's redundant with another pass.
- **First-frame splash/timer systems** must clamp their per-frame dt
  to ~0.25 s. A 20 s stall should not count as "time served".

## Files involved in the fix

- `src/ui/resource_icons.rs` — per-frame budget + drop redundant RGB pass
- `src/ui/launch/splash.rs` — clamp `MAX_SPLASH_FRAME_DT_S`
- `src/ui/launch/splash.rs::tests::splash_timer_clamps_first_frame_stall_delta`
  — regression test
- `src/ui/resource_icons.rs`'s 6 existing tests — still pass post-fix

## Measurement baseline (target machine: Intel Arc B580, DX12)

- Before fix: **20.2 s** from window creation to first PostUpdate
- After fix: **~1.1 s** (window creation → first PostUpdate)
- Splash visible lifetime: unchanged at ~3 s after the splash first
  paints

## See also

- `.github/copilot-instructions.md` — Anti-stall rules for
  `asset loading` and `splash / launch timer` subsystems (added 2026-08-05)
- `memories/repo/MEMORY.md` — memory index
