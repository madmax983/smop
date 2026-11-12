#!/usr/bin/env bash
# Examples check. Each file in examples/ must:
# - be an [[example]] target with required-features,
# - build with only those features (so the list is complete),
# - have a `cargo run --example <name>` line in README.md.
# Then `cargo build --examples --all-features` builds all of them.
# Needs jq. Run: bash scripts/examples.sh
set -euo pipefail
cd "$(dirname "$0")/.."

meta=$(cargo metadata --format-version 1 --no-deps)
targets=$(jq -r '.packages[] | select(.name == "smop") | .targets[]
  | select(.kind == ["example"])
  | "\(.name)\t\(.src_path)\t\(.["required-features"] // [] | join(","))"' <<<"$meta")

fail=0
err() { echo "::error::$1"; fail=1; }

for file in examples/*.rs; do
  name=$(basename "$file" .rs)
  grep -q "^$name	" <<<"$targets" || err "$file: no [[example]] target"
done

while IFS=$'\t' read -r name src features; do
  [ -n "$name" ] || continue
  [ -n "$features" ] || { err "example $name: no required-features"; continue; }
  grep -q "cargo run --example $name\b" README.md || err "example $name: not in README.md"
  echo "::group::example $name (features: $features)"
  cargo build -p smop --no-default-features --features "$features" --example "$name" \
    || err "example $name: does not build with only $features ($src)"
  echo "::endgroup::"
done <<<"$targets"

cargo build -p smop --examples --all-features || err "cargo build --examples failed"

[ "$fail" = 0 ] && echo "examples: $(grep -c . <<<"$targets") ok"
exit "$fail"
