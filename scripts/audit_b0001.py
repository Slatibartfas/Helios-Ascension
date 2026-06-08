#!/usr/bin/env python3
"""Audit helios src/ for Bevy 0.18 B0001 query conflicts.

Background
----------
Bevy 0.18 raises error **B0001** at schedule time when a system declares
two `Query<...>` system parameters that both yield access to the same
component (read+mut, mut+mut, or even read+read in some cases when the
planner cannot prove the queries are disjoint). The error is *runtime* —
`cargo build` and `cargo test` do not catch it, only `cargo run` does.

Canonical fix
-------------
Fold the two queries into a single `Query<(Entity, &mut T)>` and call
`iter()` then `get_mut(entity)` in sequence. Acceptable alternatives:
`ParamSet<(Query<...>, ...)>` for parallel access, or filters that are
statically disjoint (e.g. `With<A>` vs `Without<A>`).

Usage
-----
    python3 scripts/audit_b0001.py [--strict] [PATH ...]

PATH defaults to ``src/``. Exits 0 in normal mode (prints findings,
non-blocking), exits 1 in ``--strict`` mode if any `risk` candidates
are found.

Severity model
--------------
- ``risk``: multiple Query params touch the same component, at least one
  is `&mut`, no `ParamSet` wrap, and the audit could not prove the
  filters disjoint. These are the patterns most likely to panic.
- ``info``: multiple Query params touch the same component but all
  accesses are immutable, or the queries use disjoint filters /
  `ParamSet`. Probably fine, listed for human review.
"""
from __future__ import annotations

import argparse
import re
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path

QUERY_RE = re.compile(r"\bQuery\s*<((?:[^<>]|<[^<>]*>)*)>")


def split_top_level(s: str) -> list[str]:
    parts: list[str] = []
    cur: list[str] = []
    dp = da = 0
    for ch in s:
        if ch == "(":
            dp += 1
        elif ch == ")":
            dp -= 1
        elif ch == "<":
            da += 1
        elif ch == ">":
            da -= 1
        elif ch == "," and dp == 0 and da == 0:
            parts.append("".join(cur).strip())
            cur = []
            continue
        cur.append(ch)
    if cur:
        parts.append("".join(cur).strip())
    return parts


@dataclass
class QueryShape:
    accesses: list[tuple[str, bool, str]] = field(default_factory=list)
    with_: list[str] = field(default_factory=list)
    without_: list[str] = field(default_factory=list)


def parse_query(q_inner: str) -> QueryShape:
    shape = QueryShape()
    for tok in split_top_level(q_inner):
        t = tok.strip()
        if not t:
            continue
        if t.startswith("(") and t.endswith(")"):
            sub = parse_query(t[1:-1])
            shape.accesses.extend(sub.accesses)
            shape.with_.extend(sub.with_)
            shape.without_.extend(sub.without_)
            continue
        m = re.match(r"^With\s*<\s*([\w:]+)\s*>$", t)
        if m:
            shape.with_.append(m.group(1))
            continue
        m = re.match(r"^Without\s*<\s*([\w:]+)\s*>$", t)
        if m:
            shape.without_.append(m.group(1))
            continue
        m = re.match(r"^(Changed|Added)\s*<\s*([\w:]+)\s*>$", t)
        if m:
            shape.with_.append(m.group(2))
            continue
        if t.startswith(("Or<", "AnyOf<", "NoneOf<", "Ref<", "Deref<", "Try<")):
            continue
        m = re.match(r"^&mut\s+([\w:]+(?:<[^<>]*>)?)$", t)
        if m:
            shape.accesses.append((m.group(1), True, t))
            continue
        m = re.match(r"^&\s*([\w:]+(?:<[^<>]*>)?)$", t)
        if m:
            shape.accesses.append((m.group(1), False, t))
            continue
        m = re.match(r"^Has<([\w:]+)>$", t)
        if m:
            shape.with_.append(m.group(1))
            continue
        if t in ("Entity", "()", ""):
            continue
    return shape


def find_functions(text: str) -> list[tuple[str, int, int, int, int]]:
    """Return (name, fn_start, paren_open, paren_close, body_end) tuples.

    `fn_start` is the index of the `f` in `fn name`. `body_end` is one
    past the closing `}` of the function body. Filters out `fn` matches
    that are not Rust function definitions (no `(` right after the
    name).
    """
    spans: list[tuple[str, int, int, int, int]] = []
    fn_re = re.compile(r"\bfn\s+([a-zA-Z_][a-zA-Z0-9_]*)")
    n = len(text)
    for m in fn_re.finditer(text):
        name = m.group(1)
        i = m.end()
        while i < n and text[i] in " \t\n\r":
            i += 1
        if i >= n or text[i] != "(":
            continue
        paren_open = i
        depth = 0
        while i < n:
            ch = text[i]
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
                if depth == 0:
                    paren_close = i
                    break
            i += 1
        else:
            continue
        while i < n and text[i] != "{":
            i += 1
        if i >= n:
            continue
        depth = 0
        while i < n:
            ch = text[i]
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    spans.append((name, m.start(), paren_open, paren_close, i + 1))
                    break
            i += 1
    return spans


def filters_disjoint(a: QueryShape, b: QueryShape) -> bool:
    wa, wb = set(a.with_), set(b.with_)
    xa, xb = set(a.without_), set(b.without_)
    # Statically disjoint if a has With<X> and b has Without<X> (or vice versa)
    if wa & xb or wb & xa:
        return True
    return False


def audit_file(path: Path) -> list[dict]:
    text = path.read_text(encoding="utf-8", errors="ignore")
    findings: list[dict] = []
    for name, fn_start, paren_open, paren_close, body_end in find_functions(text):
        params_text = text[paren_open + 1 : paren_close]
        paramset = "ParamSet" in text[paren_open:paren_close]
        queries: list[tuple[str, QueryShape]] = []
        for pt in split_top_level(params_text):
            pt = pt.strip()
            if not pt:
                continue
            qm = QUERY_RE.search(re.sub(r"\bpub\s+", "", pt))
            if not qm:
                continue
            queries.append((pt, parse_query(qm.group(1))))
        if not queries:
            continue
        comp_to_qs: dict[str, list[tuple[QueryShape, str, bool, str]]] = defaultdict(list)
        for pt, shape in queries:
            for comp, is_mut, full in shape.accesses:
                comp_to_qs[comp].append((shape, pt, is_mut, full))
        line_no = text[:fn_start].count("\n") + 1
        for comp, qlist in comp_to_qs.items():
            if len(qlist) < 2:
                continue
            has_mut = any(is_mut for _, _, is_mut, _ in qlist)
            disjoint = any(
                filters_disjoint(a, b) for a, _, _, _ in qlist for b, _, _, _ in qlist if a is not b
            )
            if has_mut and not paramset and not disjoint:
                severity = "risk"
            else:
                severity = "info"
            findings.append(
                {
                    "file": str(path),
                    "function": name,
                    "line": line_no,
                    "component": comp,
                    "has_mut": has_mut,
                    "paramset": paramset,
                    "disjoint": disjoint,
                    "severity": severity,
                    "queries": [pt for _, pt, _, _ in qlist],
                }
            )
    return findings


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("paths", nargs="*", default=["src"], help="Paths to audit (default: src)")
    ap.add_argument(
        "--strict",
        action="store_true",
        help="Exit 1 if any 'risk' candidates are found (CI mode).",
    )
    args = ap.parse_args()
    findings: list[dict] = []
    for raw in args.paths:
        root = Path(raw)
        for path in sorted(root.rglob("*.rs")):
            findings.extend(audit_file(path))
    if not findings:
        print("No multi-Query-same-component patterns found.")
        return 0
    by_fn: dict[tuple[str, str, int], list[dict]] = defaultdict(list)
    for f in findings:
        by_fn[(f["file"], f["function"], f["line"])].append(f)
    risk_total = sum(1 for fs in by_fn.values() for f in fs if f["severity"] == "risk")
    print(f"=== B0001 audit ===")
    print(f"Functions with overlapping-component Query params: {len(by_fn)} (risk: {risk_total})")
    print()
    for (file, fn, line), group in sorted(by_fn.items(), key=lambda x: (x[0][0], x[0][2])):
        sev = max(f["severity"] for f in group)
        badge = {"risk": "[RISK]", "info": "[info]"}.get(sev, "[?]")
        print(f"{badge} {file}:{line}  fn {fn}")
        for f in group:
            tags: list[str] = []
            if f["has_mut"]:
                tags.append("mut")
            if f["paramset"]:
                tags.append("ParamSet")
            if f["disjoint"]:
                tags.append("disjoint-filters")
            tag_str = f"  <{' '.join(tags) or 'no-mut'}>"
            print(f"  - {f['component']}{tag_str}")
            for q in f["queries"]:
                q_disp = q if len(q) <= 130 else q[:127] + "..."
                print(f"      {q_disp}")
        print()
    if args.strict and risk_total > 0:
        print(f"FAIL: {risk_total} B0001-risk candidate(s).")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
