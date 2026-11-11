#!/usr/bin/env python3
"""Drift scan, feature table: every feature in Cargo.toml (not `default`) must have
a row in the CLAUDE.md table (the `## Features` section). Every `dep:<crate>` a feature enables must be named
in its row. The row must not name an optional crate that the feature does not
enable, directly or through other features. Exit 1 on drift."""
import re, sys, pathlib
root = pathlib.Path(__file__).resolve().parent.parent
cargo = (root / "Cargo.toml").read_text()
feat = cargo.split("[features]")[1].split("\n[")[0]
actual, implies = {}, {}
for m in re.finditer(r'^([\w-]+)\s*=\s*\[(.*?)\]', feat, re.S | re.M):
    actual[m[1]] = set(re.findall(r'dep:([\w-]+)', m[2]))
    implies[m[1]] = set(re.findall(r'"([\w-]+)"', m[2]))
optional = set().union(*actual.values())


def reach(f, seen=None):
    """Returns the optional crates that feature f enables, also through other features."""
    seen = seen if seen is not None else set()
    if f in seen or f not in actual:
        return set()
    seen.add(f)
    return actual[f].union(*(reach(g, seen) for g in implies[f]))


drift = []
rows = set()
lines = (root / "CLAUDE.md").read_text().splitlines()
start = lines.index("## Features")
end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith("## ")), len(lines))
for n, line in enumerate(lines[start:end], start + 1):
    m = re.match(r'\| `([\w-]+)`\s*\| (.*?)\s*\|', line)
    if not m or m[1] not in actual: continue
    rows.add(m[1])
    named = {t for t in re.findall(r'[\w-]+', m[2]) if t in optional}
    for c in sorted(actual[m[1]] - named): drift.append(f"CLAUDE.md:{n} `{m[1]}` row omits dep {c}")
    for c in sorted(named - reach(m[1])): drift.append(f"CLAUDE.md:{n} `{m[1]}` row names {c}, not enabled by feature")
for f in sorted(set(actual) - rows - {"default"}): drift.append(f"CLAUDE.md: no row for feature `{f}`")
print(f"feature-table drift: {len(drift)}")
print("\n".join("DRIFT " + d for d in drift))
sys.exit(1 if drift else 0)
