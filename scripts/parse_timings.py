"""Parse cargo --timings HTML and print slowest compilation units."""
import json
import re
import sys
from pathlib import Path

path = Path(sys.argv[1] if len(sys.argv) > 1 else r"target/cargo-timings")
if path.is_dir():
    files = sorted(path.glob("cargo-timing-*.html"), key=lambda p: p.stat().st_mtime, reverse=True)
    if not files:
        print("no timing files")
        sys.exit(1)
    path = files[0]

html = path.read_text()
m = re.search(r"const UNIT_DATA = (\[[\s\S]*?\]);", html)
units = json.loads(m.group(1))

# total wall time (max start+duration)
total = max((u["start"] + u["duration"] for u in units), default=0.0)
sum_dur = sum(u["duration"] for u in units)
print(f"# File: {path.name}")
print(f"# Units: {len(units)}  wall={total:.1f}s  sum={sum_dur:.1f}s")
print()

print("## Slowest units (top 30)")
print(f"{'dur(s)':>8}  {'rmeta':>8}  {'start':>7}  {'mode':<10}  name / target")
for u in sorted(units, key=lambda x: -x["duration"])[:30]:
    target = u.get("target", "") or ""
    print(
        f"{u['duration']:>8.2f}  {u['rmeta_time']!s:>8}  "
        f"{u['start']:>7.2f}  {u['mode']:<10}  {u['name']} {target}"
    )

print()
print("## Non-our-crate units (deps)")
ours = {"helios_ascension"}
dep_units = [u for u in units if u["name"] not in ours]
dep_total = sum(u["duration"] for u in dep_units)
print(f"# {len(dep_units)} dep units, sum={dep_total:.1f}s")
print(f"{'dur(s)':>8}  {'rmeta':>8}  name")
for u in sorted(dep_units, key=lambda x: -x["duration"])[:20]:
    print(f"{u['duration']:>8.2f}  {u['rmeta_time']!s:>8}  {u['name']}")