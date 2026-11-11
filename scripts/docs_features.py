#!/usr/bin/env python3
"""Drift scan, feature table: every `dep:<crate>` a feature enables in Cargo.toml
must be named in that feature's row of the CLAUDE.md table, and the row must not
name an optional crate the feature does not enable. Exit 1 on drift."""
import re, sys, pathlib
root = pathlib.Path(__file__).resolve().parent.parent
cargo = (root / "Cargo.toml").read_text()
feat = cargo.split("[features]")[1].split("\n[")[0]
actual = {}
for m in re.finditer(r'^([a-z-]+)\s*=\s*\[(.*?)\]', feat, re.S | re.M):
    actual[m[1]] = set(re.findall(r'dep:([\w-]+)', m[2]))
optional = set().union(*actual.values())
drift = []
for n, line in enumerate((root / "CLAUDE.md").read_text().splitlines(), 1):
    m = re.match(r'\| `(\w+)` \| (.*?) \|', line)
    if not m or m[1] not in actual: continue
    named = {t for t in re.findall(r'[\w-]+', m[2]) if t in optional}
    for c in sorted(actual[m[1]] - named): drift.append(f"CLAUDE.md:{n} `{m[1]}` row omits dep {c}")
    for c in sorted(named - actual[m[1]]): drift.append(f"CLAUDE.md:{n} `{m[1]}` row names {c}, not enabled by feature")
print(f"feature-table drift: {len(drift)}")
print("\n".join("DRIFT " + d for d in drift))
sys.exit(1 if drift else 0)
