#!/usr/bin/env python3
"""Audit helios src/ for hardcoded `Color32::from_*` literals outside the theme.

Background
----------
The Helios UI uses a single source of truth for colour — `src/ui/theme.rs` —
to keep the dark-navy "Tactical OS" aesthetic coherent across panels. To stop
the regression that produced 96+ hardcoded `Color32::from_rgb(...)` calls
across the panel code, the GRA-54 PR swept the codebase and promoted those
constants into `theme.rs`. GRA-58 (this script's owner) adds a CI guard so
future commits cannot reintroduce the same drift.

What it flags
-------------
Any call to a `Color32` constructor (`from_rgb`, `from_rgba_premultiplied`,
`from_rgba_unmultiplied`, `from_gray`, `from_hex`, `from_additive_lum`, …) in
a `.rs` file under `src/` *except* `src/ui/theme.rs`. Theme.rs is the only
authorised home for raw colour literals.

Usage
-----
    # Informational — print every violation, exit 0.
    python3 scripts/audit_color32_literals.py

    # CI mode — fail if any violation is not already in the baseline.
    python3 scripts/audit_color32_literals.py --strict \\
        --baseline scripts/audit_color32_literals_baseline.txt

    # Regenerate the baseline after a cleanup PR.
    python3 scripts/audit_color32_literals.py --emit-baseline \\
        > scripts/audit_color32_literals_baseline.txt

Baseline format
---------------
One violation per line: ``<file>  <literal>``. File paths are
repo-relative. The literal is the matched constructor call's first line
as it appears in source, single-trimmed, with arbitrary trailing text
replaced by ``…`` when the call is longer than 120 chars.

Baseline dedup
--------------
A single ``<file>  <literal>`` entry tolerates every occurrence of that
literal in that file — line-number drift in surrounding code does not
trigger a new finding. Truly new literals (a different file or a
different source text) still fail ``--strict``. This absorbs the
``<file>:<line>`` drift cascade that bit main CI four times in 24h
(GRA-386 → GRA-390 → GRA-403 → GRA-782).
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# egui::Color32 constructors that the audit treats as "raw literal colour".
# The list is intentionally permissive — if a new constructor is added to
# the egui API it will get flagged by the regex and we can decide whether
# to keep it in the allowlist (e.g. `from_black` / `from_white` are also
# raw literals, just shortcuts to 0 or 255).
CONSTRUCTOR_RE = re.compile(
    r"\b(Color32|egui::Color32)::("
    r"from_rgb|from_rgba_premultiplied|from_rgba_unmultiplied|"
    r"from_gray|from_hex|from_hex_additive|from_black|from_white|"
    r"from_additive_lum|from_lum_alpha|from_srgba|from_srgba_premultiplied|"
    r"from_rgba|from_rgb_additive|from_srgb"
    r")\b"
)

# Files exempt from the audit. Add a new entry only with a clear justification
# (e.g. an asset loader that reads colours from a binary blob, or a
# non-UI Bevy render pass).
DEFAULT_ALLOWLIST = {
    "src/ui/theme.rs",
}

# Cap on how much of the matched literal we print to keep the audit report
# readable. Anything longer is truncated with a trailing ellipsis.
LITERAL_DISPLAY_CAP = 120


def find_constructors(text: str) -> list[tuple[int, str]]:
    """Return ``[(line_number_1_based, matched_substring), ...]`` for every
    `Color32::from_*` call in ``text``.
    """
    hits: list[tuple[int, str]] = []
    for m in CONSTRUCTOR_RE.finditer(text):
        line_no = text.count("\n", 0, m.start()) + 1
        hits.append((line_no, m.group(0)))
    return hits


def _relativise(path: Path) -> str:
    """Return a repo-relative POSIX path when possible, else the original."""
    try:
        return path.resolve().relative_to(Path.cwd().resolve()).as_posix()
    except ValueError:
        return path.as_posix()


def audit_file(path: Path) -> list[dict]:
    rel = _relativise(path)
    if rel in DEFAULT_ALLOWLIST:
        return []
    try:
        text = path.read_text(encoding="utf-8")
    except (UnicodeDecodeError, OSError) as exc:
        print(f"warning: could not read {rel}: {exc}", file=sys.stderr)
        return []
    findings: list[dict] = []
    for line_no, matched in find_constructors(text):
        line_text = text.splitlines()[line_no - 1].strip()
        if len(line_text) > LITERAL_DISPLAY_CAP:
            line_text = line_text[: LITERAL_DISPLAY_CAP - 1] + "…"
        findings.append(
            {
                "file": rel,
                "line": line_no,
                "constructor": matched,
                "source": line_text,
            }
        )
    return findings


def render_baseline_line(f: dict) -> str:
    """Serialise one finding in the baseline-file format.

    Format: ``<file>  <literal>``. Line number is intentionally dropped
    so that line-number drift in surrounding code does not break the
    audit. See module docstring "Baseline dedup" for the rationale.
    """
    return f"{f['file']}  {f['source']}"


def _finding_key(f: dict) -> tuple[str, str]:
    """Stable dedup key for a finding: ``(file, source)``.

    Line number is intentionally excluded so that line-number drift in
    surrounding code does not produce a "new" finding for an existing
    literal.
    """
    return (f["file"], f["source"])


def load_baseline(path: Path) -> set[tuple[str, str]]:
    """Load the baseline as a set of ``(file, source)`` keys.

    The on-disk format is ``<file>  <literal>``; entries are split at
    the first run of two-or-more spaces. Blank lines and lines starting
    with ``#`` are skipped.
    """
    if not path.exists():
        return set()
    entries: set[tuple[str, str]] = set()
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if "  " not in line:
            # Malformed entry — skip defensively rather than fail the
            # whole audit on a typo.
            continue
        file_part, source_part = line.split("  ", 1)
        entries.add((file_part, source_part.strip()))
    return entries


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument(
        "paths",
        nargs="*",
        default=["src"],
        help="Paths to audit (default: src). Files matching the allowlist are skipped.",
    )
    ap.add_argument(
        "--strict",
        action="store_true",
        help="Exit 1 if any new violation is found (CI mode).",
    )
    ap.add_argument(
        "--baseline",
        type=Path,
        default=None,
        help="Path to a baseline file. In --strict mode, violations already in "
        "the baseline are tolerated; anything new fails the build. Without "
        "--strict, the baseline only filters the printed report.",
    )
    ap.add_argument(
        "--emit-baseline",
        action="store_true",
        help="Write the current set of violations to stdout in baseline-file "
        "format and exit 0. Combine with shell redirection to refresh the "
        "baseline file.",
    )
    args = ap.parse_args()

    repo_root = Path.cwd().resolve()
    findings: list[dict] = []
    for raw in args.paths:
        root = Path(raw)
        if not root.exists():
            print(f"warning: {raw} does not exist, skipping", file=sys.stderr)
            continue
        for path in sorted(root.rglob("*.rs")):
            findings.extend(audit_file(path))

    if args.emit_baseline:
        # Dedupe by (file, source) so multiple occurrences of the same
        # literal in a file collapse to a single baseline entry. Without
        # this the baseline would grow unboundedly and obscure genuinely
        # new entries; with it, one entry tolerates every occurrence.
        seen_keys: set[tuple[str, str]] = set()
        deduped_findings: list[dict] = []
        for f in findings:
            key = _finding_key(f)
            if key in seen_keys:
                continue
            seen_keys.add(key)
            deduped_findings.append(f)
        header = [
            "# Color32 literal audit baseline.",
            "# One violation per line: <file>  <literal>",
            "# Regenerate with: python3 scripts/audit_color32_literals.py --emit-baseline",
            "# Move entries out as they're promoted to src/ui/theme.rs constants.",
            "",
        ]
        print("\n".join(header + [render_baseline_line(f) for f in deduped_findings]))
        return 0

    baseline = load_baseline(args.baseline) if args.baseline else set()

    # Match by (file, source) so line-number drift is absorbed.
    baseline_keys = baseline
    finding_keys = {_finding_key(f) for f in findings}
    new_keys = finding_keys - baseline_keys

    new_findings: list[dict] = []
    known_findings: list[dict] = []
    for f in findings:
        if _finding_key(f) in baseline_keys:
            known_findings.append(f)
        else:
            new_findings.append(f)

    print("=== Color32 literal audit ===")
    print(
        f"Files scanned: {sum(1 for p in Path('src').rglob('*.rs')) if Path('src').exists() else 'n/a'}"
    )
    print(f"Allowlisted: {sorted(DEFAULT_ALLOWLIST)}")
    print(f"Baseline entries: {len(baseline)} (from {args.baseline or '<none>'})")
    print(f"Total findings: {len(findings)} (new: {len(new_findings)}, in baseline: {len(known_findings)})")
    print()

    if not findings:
        print("No hardcoded Color32 literals found outside src/ui/theme.rs.")
    else:
        for f in sorted(findings, key=lambda x: (x["file"], x["line"])):
            tag = "[NEW]" if _finding_key(f) not in baseline_keys else "[ok ]"
            print(f"{tag} {f['file']}:{f['line']}  {f['source']}")

    if args.strict and new_findings:
        print()
        print(
            f"FAIL: {len(new_keys)} new Color32 literal(s) outside src/ui/theme.rs. "
            "Promote them to a theme.rs constant and remove the entry from "
            f"{args.baseline} to clear this."
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
