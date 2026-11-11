#!/usr/bin/env bash
# Checks smop's public API for types of crates that smop does not re-export
# on purpose (see "API boundaries" in CLAUDE.md).
#
# Rustdoc links each type to the docs of its crate. This script documents
# smop and all its dependencies, direct and transitive. Then it reads smop's
# pages and finds links to crates that are not on the allow list.
#
# Not checked: #[doc(hidden)] items and the expansion of macros.
set -euo pipefail
cd "$(dirname "$0")/.."

# Re-exported on purpose: their major releases are smop major releases.
# chrono: only the date types that `time` re-exports. regex: all of it, as
# `smop::re`. The other names are internal crates of the allowed crates.
allowed="anyhow clap clap_builder clap_derive serde serde_core serde_derive
serde_json toml chrono regex"

target=$(cargo metadata --no-deps --format-version 1 | jq -r .target_directory)

# Rustdoc links to a crate only if the docs of that crate exist when smop's
# docs start. Cargo documents crates in parallel, so document all crates
# first, then document smop again on its own.
cargo doc -p smop --all-features --quiet
rm -rf "$target/doc/smop" "$target/doc/src/smop"
cargo doc -p smop --no-deps --all-features --quiet

python3 -I - "$target/doc" "$allowed" <<'PY'
import os, re, sys

doc, allowed = sys.argv[1], set(sys.argv[2].split())
ok = allowed | {"smop", "std", "core", "alloc", "proc_macro"}
# Implementor lists and blanket impls show other crates' types, not smop's API.
cut = re.compile(r'id="(?:foreign-impls|implementors|blanket-implementations)"')
href = re.compile(r'href="([^"#]+)')
src = re.compile(r'<a class="src[^"]*" href="([^"#]+)')
name = re.compile(r"[a-z0-9_]+")


def crate_of(link, base):
    """Returns the crate that a link points to, or None."""
    if link.startswith("https://docs.rs/"):
        return link.split("/")[3].replace("-", "_")
    if link.startswith("https://doc.rust-lang.org/"):
        return "std"
    if "://" in link or link.startswith("mailto:"):
        return None
    parts = os.path.relpath(os.path.normpath(os.path.join(base, link)), doc).split(os.sep)
    if parts[0] == "src":
        parts = parts[1:]
    if len(parts) < 2 or not name.fullmatch(parts[0]):
        return None
    return parts[0]


hits = []
for root, _, files in os.walk(os.path.join(doc, "smop")):
    for file in files:
        if not file.endswith(".html"):
            continue
        path = os.path.join(root, file)
        page = open(path, encoding="utf-8").read()
        shown = os.path.relpath(path, doc)
        # A page for an item that smop re-exports from another crate.
        first = src.search(page)
        owner = crate_of(first.group(1), root) if first else "smop"
        if owner in allowed:
            continue
        if owner not in ok:
            hits.append(f"{shown}: re-exports an item of {owner}")
            continue
        page = cut.split(page, maxsplit=1)[0]
        for link in sorted(set(href.findall(page))):
            crate = crate_of(link, root)
            if crate and crate not in ok:
                hits.append(f"{shown}: {crate} ({link})")

if hits:
    print("Types of other crates in smop's public API:")
    print("\n".join(sorted(hits)))
    print("A link in a doc comment to the docs of a crate also shows here.")
    sys.exit(1)
print("api boundary: ok")
PY
