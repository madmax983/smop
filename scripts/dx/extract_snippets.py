#!/usr/bin/env python3 -I
"""Extract every ```rust block from the given markdown files into a scratch crate.

usage: extract_snippets.py OUT_DIR SMOP_PATH FILE.md...
Blocks without `fn main` are wrapped in `fn main() -> Result<()> { ...; Ok(()) }`
with the prelude imported, the way a reader would paste a fragment into a script.
Writes OUT_DIR/Cargo.toml, OUT_DIR/src/bin/<id>.rs and OUT_DIR/index.tsv (id, file, line).
"""
import os, re, sys

out, smop, *files = sys.argv[1:]
os.makedirs(f"{out}/src/bin", exist_ok=True)
open(f"{out}/Cargo.toml", "w").write(f"""[package]
name = "snippets"
version = "0.0.0"
edition = "2024"

[dependencies]
smop = {{ path = "{smop}" }}
""")
index = []
for f in files:
    lines = open(f).read().split("\n")
    i = 0
    while i < len(lines):
        m = re.match(r"^```rust\b(.*)$", lines[i])
        if m:
            start, body = i + 1, []
            i += 1
            while not lines[i].startswith("```"):
                body.append(lines[i]); i += 1
            src = "\n".join(body)
            if src.startswith("#!"):   # cargo-script frontmatter: needs nightly cargo -Zscript
                i += 1; continue
            sid = f"{re.sub(r'[^a-z]', '', os.path.basename(f).lower().split('.')[0])}_{start}"
            if not re.search(r"\bfn main\b", src):
                src = "use smop::prelude::*;\nfn main() -> Result<()> {\n" + src + "\nOk(())\n}\n"
            open(f"{out}/src/bin/{sid}.rs", "w").write("#![allow(unused)]\n" + src + "\n")
            index.append(f"{sid}\t{f}\t{start}")
        i += 1
open(f"{out}/index.tsv", "w").write("\n".join(index) + "\n")
