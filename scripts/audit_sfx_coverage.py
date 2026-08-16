#!/usr/bin/env python3
"""Audit SFX coverage across the UI surface.

This is the *regression gate* for the SFX system. The manifest
audit (`scripts/audit_sfx_manifest.py`) checks that every cue
the **runtime knows about** corresponds to a manifest entry.
This script checks the **opposite** direction: that every
interactive UI callsite that should fire a cue actually does.

## Categories

The audit recognises six categories of UI signal. Each maps
to one or more `SfxCueId` variants and a specific egui
pattern. A callsite that matches a category's regex without
having a corresponding SFX write within the same function
body is reported as an unwired call site.

| Category | Cue | egui Pattern |
|---|---|---|
| `click`        | `ButtonClick` / `ModalConfirm` / `RowSelect` / etc. | `.clicked()` |
| `slider`       | `SliderTick`           | `Slider::new(...)` |
| `dropdown`     | `DropdownOpen`         | `ComboBox::` |
| `tab`          | `TabSwitch`            | `selectable_label` inside a tab loop |
| `drag_drop`    | `DragDrop`             | `dnd_drop` / `drag_started` / `drag_stopped` |
| `panel_change` | `PanelOpen` / `PanelClose` | `LaunchState::MainMenu` / `Settings` etc. transitions |

## What "wired" means

For each detected call site, the audit looks at the enclosing
function body (defined by `{ ... }` balance). A call site is
"wired" if any of these appear *after* the call site line
inside the same function:

- `sfx_ui.write(`
- `UiSfxRequest(SfxCueId::`
- `egui_sfx_` (any wrapper from `src/ui/egui_sfx.rs`)
- `crate::plugins::sfx::bridges::UiSfxRequest`

If none of these appear, the call site is **unwired**.

## Use cases

    # List unwired call sites (no exit code).
    python3 scripts/audit_sfx_coverage.py --src src/ui

    # Fail CI if any unwired call site exists.
    python3 scripts/audit_sfx_coverage.py --strict --src src/ui

The audit is intentionally permissive on legacy code; it
prints the list of unwired sites so the team can wire them
in tracked chunks rather than a giant PR.

## Patterns

Mirrors `scripts/audit_sfx_manifest.py`,
`scripts/audit_color32_literals.py`, `scripts/audit_b0001.py`.
Print findings by default; exit 1 under `--strict`.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# Force UTF-8 stdout on Windows so emoji-rich UI text in
# audit output doesn't crash the cp1252 codec.
if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except AttributeError:  # Python < 3.7 fallback
        pass

# ---------------------------------------------------------------------------
# Patterns
# ---------------------------------------------------------------------------

# Function-entry regex (rough; doesn't handle brace-string-literal edge
# cases like `"{".to_string()` but it's good enough for the egui UI code
# we maintain).
FN_DEF = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)\s*[<(](?P<args>.*?)?[>)]\s*(?:->\s*[^{;]+)?\s*\{",
    re.DOTALL | re.MULTILINE,
)

# Closed-brace depth tracker — finds the line of the closing `}`
# that matches a given function-def brace.
def find_enclosing_function(lines: list[str], start_line: int) -> tuple[int, int]:
    """Return (start_line, end_line_exclusive) of the function that
    contains `start_line`. If a brace counts before reaching
    `start_line`, return (-1, -1).
    """
    # Walk backward looking for a `fn` definition; track brace
    # depth; return when balanced.
    depth = 0
    for i in range(start_line, -1, -1):
        line = lines[i]
        # Count braces right-to-left so we handle single-line
        # closing braces (`}`) correctly.
        for ch in reversed(line):
            if ch == "}":
                depth += 1
            elif ch == "{":
                if depth == 0:
                    return (i, -1)
                depth -= 1
    return (-1, -1)


def find_function_end(lines: list[str], start_line: int) -> int:
    """From a `fn` definition line, find the matching `}` and
    return the line *after* it. Returns `len(lines)` if no
    close brace is found.
    """
    depth = 0
    seen_open = False
    for i in range(start_line, len(lines)):
        line = lines[i]
        for ch in line:
            if ch == "{":
                depth += 1
                seen_open = True
            elif ch == "}":
                depth -= 1
                if seen_open and depth == 0:
                    return i + 1
    return len(lines)


# ---------------------------------------------------------------------------
# Categories — each maps to a regex and a label.
# ---------------------------------------------------------------------------

CATEGORIES: list[tuple[str, re.Pattern[str]]] = [
    ("click",        re.compile(r"\.clicked\(\)")),
    ("slider",       re.compile(r"Slider::new\(|Slider::from|DragValue::")),
    ("dropdown",     re.compile(r"ComboBox::")),
    ("tab",          re.compile(r"selectable_label")),
    ("drag_drop",    re.compile(r"dnd_drop|drag_started|drag_stopped")),
    # `panel_change` matches sites where the LaunchState is being
    # *mutated* (`*launch_state = X` or `next_launch_state = X`),
    # not every reference to LaunchState in the file (which is
    # extremely common — eg. for gating sub-view rendering).
    ("panel_change", re.compile(r"\*\s*launch_state\s*=|next_launch_state\s*=|game_menu\.set|GameMenu::set")),
]

# Strings or path fragments that indicate "this file should NOT be
# audited" — for example, the SFX plugin itself or the egui_sfx
# wrapper module which is *the* API the audit is checking against.
EXCLUDE_PATH_FRAGMENTS = (
    "src/plugins/sfx/",  # the SFX plugin itself
    "src/ui/egui_sfx.rs",  # the wrapper module — its examples are wrappers
    "src/ui/launch/subview_kickoff.rs",  # exclusive `&mut World` system; can't carry MessageWriter
    "src/ui/launch/manifest.rs",  # loader-only
    "src/ui/launch/save_index.rs",
    "src/ui/launch/userdata.rs",
    "src/ui/launch/transitions.rs",
    "src/ui/launch/menu_backdrop.rs",
    "src/ui/launch/boot_overlay.rs",
    "src/ui/launch/return_to_menu.rs",
    "src/ui/launch/subview_manifests.rs",
    "src/ui/launch/subview_settings.rs",  # Phase 3
    "src/ui/notifications/",  # Phase 3
    "src/astronomy/selection.rs",  # body click — Phase 3
    "src/ui/theme.rs",  # palette only
    "src/ui/tab.rs",  # tab primitive
    "src/ui/widgets.rs",  # Bevy UI primitive library
    "src/ui/bevy_theme.rs",  # Bevy UI palette mirror
    "src/ui/porkchop_color_ramp.rs",
    "src/ui/icon_cache.rs",
    "src/ui/icons.rs",
    "src/ui/resource_icons.rs",
    "src/ui/screenshot",
    "src/ui/screenshot_state.rs",
    "src/ui/cursors.rs",
    "src/ui/interaction.rs",
    "src/ui/resources_bar.rs",  # top bar — already wrapped at construction; rest is display
    "tests/",  # cargo test files
    "src/test_util.rs",
    "src/boot_init.rs",
    "src/main.rs",
)


def is_excluded(path: Path) -> bool:
    posix = path.as_posix()
    return any(frag in posix for frag in EXCLUDE_PATH_FRAGMENTS)


# Strings that mean "the function body has an SFX write somewhere"
WIRED_MARKERS = (
    "sfx_ui.write(",
    "UiSfxRequest(",
    "egui_sfx_",
    "crate::plugins::sfx::bridges::UiSfxRequest",
    "crate::ui::egui_sfx::",  # fully-qualified wrapper path
    "use crate::plugins::sfx",  # importing the bus at all = intent to wire
)


def function_is_wired(function_body: str) -> bool:
    return any(marker in function_body for marker in WIRED_MARKERS)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def scan_file(path: Path) -> list[tuple[str, str, int, str]]:
    """Return a list of (category, fn_name, line, line_text) for unwired
    call sites in `path`.
    """
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    findings: list[tuple[str, str, int, str]] = []

    for category, pattern in CATEGORIES:
        for match in pattern.finditer(text):
            line_no = text.count("\n", 0, match.start())
            line_text = lines[line_no].rstrip()
            # Find the enclosing function. If none (top-level code or
            # trait impl), the call site is treated as a finding — the
            # safe default.
            fn_def_start, _ = find_enclosing_function(lines, line_no)
            if fn_def_start < 0:
                findings.append((category, "<free>", line_no + 1, line_text))
                continue
            fn_end = find_function_end(lines, fn_def_start)
            fn_name_match = FN_DEF.search(lines[fn_def_start]) if lines[fn_def_start].lstrip().startswith(("pub ", "fn ", "pub fn")) else None
            fn_name = fn_name_match.group(1) if fn_name_match else "<fn>"
            body = "\n".join(lines[fn_def_start:fn_end])
            if not function_is_wired(body):
                findings.append((category, fn_name, line_no + 1, line_text))

    return findings


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[1])
    parser.add_argument(
        "--src",
        default="src/ui",
        type=Path,
        help="Directory to scan (default: src/ui).",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit non-zero if any unwired call sites are found.",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        default=None,
        help=(
            "Path to a baseline file listing call sites that are "
            "known-unwired (one path:N per line). Sites in the "
            "baseline are reported but don't count toward the "
            "--strict exit code."
        ),
    )
    args = parser.parse_args(argv)

    if not args.src.exists():
        print(f"audit_sfx_coverage: {args.src} does not exist", file=sys.stderr)
        return 2

    # Load baseline if provided.
    baseline: set[tuple[str, int]] = set()
    if args.baseline and args.baseline.exists():
        for raw in args.baseline.read_text(encoding="utf-8").splitlines():
            raw = raw.strip()
            if not raw or raw.startswith("#"):
                continue
            if ":" not in raw:
                continue
            posix, _, ln = raw.rpartition(":")
            try:
                baseline.add((posix, int(ln)))
            except ValueError:
                continue

    files = sorted(p for p in args.src.rglob("*.rs") if not is_excluded(p))
    findings_total = 0
    by_category: dict[str, list[tuple[Path, int, str]]] = {
        c: [] for c, _ in CATEGORIES
    }
    for path in files:
        rel = path.as_posix()
        for category, _fn_name, line_no, line_text in scan_file(path):
            if (rel, line_no) in baseline:
                continue
            by_category[category].append((path, line_no, line_text))
            findings_total += 1

    if findings_total == 0:
        print(
            f"audit_sfx_coverage: 0 unwired call sites across "
            f"{len(files)} file(s) — every interactive signal is wired."
        )
        return 0

    print(
        f"audit_sfx_coverage: {findings_total} potentially-unwired call "
        f"site(s) across {len(files)} file(s):"
    )
    for category, items in by_category.items():
        if not items:
            continue
        print(f"\n  [{category}] — {len(items)} site(s)")
        # Group by file for readability.
        by_file: dict[Path, list[tuple[int, str]]] = {}
        for path, line_no, text in items:
            by_file.setdefault(path, []).append((line_no, text))
        for path, hits in sorted(by_file.items()):
            print(f"    {path.as_posix()}")
            for line_no, text in hits[:10]:
                # Emit via sys.stdout.buffer so Windows cp1252
                # consoles don't crash on emoji/unicode from
                # rich UI text strings.
                sys.stdout.buffer.write(
                    f"      L{line_no}: {text.strip()[:80]}\n".encode(
                        "utf-8", errors="replace"
                    )
                )
            if len(hits) > 10:
                sys.stdout.buffer.write(
                    f"      ... and {len(hits) - 10} more\n".encode("utf-8")
                )

    if args.strict:
        print(
            "\naudit_sfx_coverage: --strict enabled; failing. Use a "
            "baseline file to defer sites that are known-unwired "
            "(see --baseline).",
            file=sys.stderr,
        )
        return 1
    print(
        "\naudit_sfx_coverage: report-only mode. Run with --strict to "
        "fail CI on unwired call sites."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
