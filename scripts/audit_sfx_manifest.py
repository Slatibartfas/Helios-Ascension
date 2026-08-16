#!/usr/bin/env python3
"""Audit the SFX manifest against the Rust SfxCueId enum.

The SFX system (`src/plugins/sfx/`) has two surfaces that must
stay in lockstep:

1. **`SfxCueId` enum** (`src/plugins/sfx/mod.rs`) — the
   compile-time list of every cue the runtime can play.
2. **`assets/data/sfx_manifest.ron`** — the data-driven list of
   every cue with file paths and metadata.

This script enforces the correspondence between the two so a
mismatch fails CI before the binary is built:

- Every `SfxCueId::ALL` variant must have a matching manifest
  entry (string id matches `SfxCueId::as_str_id`).
- Every manifest entry's `id` must resolve to a variant
  (otherwise the runtime silently drops it).
- Every manifest entry's `file` must resolve to an existing
  `.wav` in `assets/audio/sfx/`.

Mirrors the design of `scripts/audit_color32_literals.py` and
`scripts/audit_b0001.py` (print findings by default, exit 1
under `--strict`).

Usage
-----

    # Print findings (non-blocking, default).
    python3 scripts/audit_sfx_manifest.py

    # Exit non-zero if any findings reported.
    python3 scripts/audit_sfx_manifest.py --strict
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
SFX_MODULE = REPO_ROOT / "src" / "plugins" / "sfx" / "mod.rs"
MANIFEST_PATH = REPO_ROOT / "assets" / "data" / "sfx_manifest.ron"
SFX_DIR = REPO_ROOT / "assets" / "audio" / "sfx"


def extract_cue_ids(mod_rs: Path) -> set[str]:
    """Parse `SfxCueId::ALL` from `src/plugins/sfx/mod.rs`.

    The enum is laid out as:
        pub const ALL: &'static [SfxCueId] = &[
            Self::ButtonClick,
            Self::TabSwitch,
            ...
        ];

    We grep for `Self::<Variant>` lines inside `ALL` and then
    map each variant back to its `as_str_id` form via the
    `match` block in the same file.
    """
    if not mod_rs.exists():
        sys.exit(f"SFX module not found: {mod_rs}")

    text = mod_rs.read_text(encoding="utf-8")

    # 1. Find `SfxCueId::ALL` block.
    all_match = re.search(
        r"pub\s+const\s+ALL\s*:\s*&'static\s*\[SfxCueId\]\s*=\s*&\[(.*?)\];",
        text,
        re.DOTALL,
    )
    if not all_match:
        sys.exit(f"could not find `SfxCueId::ALL` in {mod_rs}")
    all_block = all_match.group(1)
    variants = re.findall(r"Self::(\w+)", all_block)

    # 2. Parse `as_str_id` to build variant → string id map.
    as_str_match = re.search(
        r"pub\s+fn\s+as_str_id\s*\([^)]*\)\s*->\s*&'static\s*str\s*\{(.*?)\n\s*\}",
        text,
        re.DOTALL,
    )
    if not as_str_match:
        sys.exit(f"could not find `as_str_id` body in {mod_rs}")
    as_str_body = as_str_match.group(1)
    str_ids: dict[str, str] = {}
    for line in as_str_body.splitlines():
        m = re.match(r'\s*Self::(\w+)\s*=>\s*"([^"]+)"', line)
        if m:
            str_ids[m.group(1)] = m.group(2)

    # 3. Compose the set of string ids in ALL.
    out: set[str] = set()
    for v in variants:
        if v not in str_ids:
            sys.exit(
                f"variant `Self::{v}` is in ALL but missing from "
                f"`as_str_id` — these two must stay in sync"
            )
        out.add(str_ids[v])
    return out


def parse_manifest_ids(path: Path) -> list[tuple[str, str]]:
    """Return a list of `(cue_id, file)` tuples from the manifest.

    The RON schema is one top-level tuple with a `cues:` array
    of named-field tuples. We do a small hand-rolled parse
    rather than pulling in `ron` from PyPI; the schema is
    stable and the file is small.
    """
    if not path.exists():
        sys.exit(f"manifest not found: {path}")
    text = path.read_text(encoding="utf-8")

    cues: list[tuple[str, str]] = []
    # Find the `cues: [` block.
    cues_match = re.search(r"cues:\s*\[(.*)\n\s*\]", text, re.DOTALL)
    if not cues_match:
        sys.exit(f"could not find `cues: [...]` block in {path}")
    body = cues_match.group(1)

    # Split on top-level `(` — each cue is `(id: "...", file: "...", ...)`.
    depth = 0
    start: int | None = None
    for i, ch in enumerate(body):
        if ch == "(":
            if depth == 0:
                start = i + 1
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0 and start is not None:
                cue_text = body[start:i]
                id_m = re.search(r'id:\s*"([^"]+)"', cue_text)
                file_m = re.search(r'file:\s*"([^"]+)"', cue_text)
                if id_m and file_m:
                    cues.append((id_m.group(1), file_m.group(1)))
                start = None
    return cues


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit non-zero if any findings are reported.",
    )
    args = parser.parse_args()

    rust_ids = extract_cue_ids(SFX_MODULE)
    manifest_entries = parse_manifest_ids(MANIFEST_PATH)
    manifest_ids = {cid for cid, _ in manifest_entries}

    findings: list[str] = []

    # ── 1. Every Rust variant must have a manifest entry. ─────
    missing_in_manifest = sorted(rust_ids - manifest_ids)
    if missing_in_manifest:
        findings.append(
            f"manifest is missing {len(missing_in_manifest)} cue(s) "
            f"that the Rust enum declares:\n  " + "\n  ".join(missing_in_manifest)
        )

    # ── 2. Every manifest entry must map to a Rust variant. ───
    extra_in_manifest = sorted(manifest_ids - rust_ids)
    if extra_in_manifest:
        findings.append(
            f"manifest has {len(extra_in_manifest)} cue(s) the Rust "
            f"enum doesn't know about (these will be silently dropped "
            f"at runtime — add the variant to SfxCueId in "
            f"src/plugins/sfx/mod.rs):\n  " + "\n  ".join(extra_in_manifest)
        )

    # ── 3. Every manifest `file` must resolve to a real WAV. ─
    for cue_id, filename in manifest_entries:
        file_path = SFX_DIR / filename
        if not file_path.exists():
            findings.append(
                f"manifest entry `{cue_id}` references "
                f"`{filename}` but no such file exists in "
                f"assets/audio/sfx/. Run `python3 "
                f"scripts/generate_sfx.py` or drop the WAV in place."
            )

    # ── Report ────────────────────────────────────────────────
    print(f"audit_sfx_manifest: {len(rust_ids)} Rust variants, "
          f"{len(manifest_entries)} manifest entries, "
          f"{sum(1 for _ in SFX_DIR.glob('*.wav'))} WAV file(s) on disk")

    if findings:
        for f in findings:
            print(f"\n  FINDING: {f}")
        print(f"\naudit_sfx_manifest: {len(findings)} finding(s)")
        if args.strict:
            return 1
        return 0

    print("audit_sfx_manifest: OK (manifest <-> Rust enum <-> files are in sync)")
    return 0


if __name__ == "__main__":
    sys.exit(main())