#!/usr/bin/env bash
# Snippet CI: compile every ```rust block in README.md and CLAUDE.md against this checkout
# (all features). Compile-only: snippets hit the network and the filesystem.
# Prints one PASS/FAIL line per block (file:line + first error) and a broken count.
# usage: scripts/dx/snippets.sh [--max-broken N]   (exit 1 if broken > N; default 0)
set -u
root="$(cd "$(dirname "$0")/../.." && pwd)"
max=0; [ "${1:-}" = "--max-broken" ] && max="$2"
work="${SNIPPET_DIR:-$(mktemp -d)}"
cd "$root"
python3 -I scripts/dx/extract_snippets.py "$work" "$root" README.md CLAUDE.md || exit 2
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$work/target}"
cd "$work"
cargo fetch -q 2>/dev/null
total=0; broken=0; ctx=0
while IFS=$'\t' read -r id file line; do
  total=$((total+1))
  if err=$(cargo check -q --offline --bin "$id" --message-format short 2>&1); then
    echo "PASS $file:$line"
  else
    kind=FAIL
    # undefined user-side names (User, Config, url...) = illustrative fragment, not a defect
    if [ "$(echo "$err" | grep -c '^src.*error')" = "$(echo "$err" | grep -cE 'error\[E0(425|412|422)\]')" ]; then kind=CTX; ctx=$((ctx+1)); else broken=$((broken+1)); fi
    echo "$kind $file:$line  $(echo "$err" | grep -m1 -E 'error' | sed "s#$work/src/bin/##")"
  fi
done < index.tsv
echo "snippets: total=$total broken=$broken needs_context=$ctx"
[ "$broken" -le "$max" ]
