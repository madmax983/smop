#!/usr/bin/env bash
# Drift scan. Every `module::symbol` named in README/CLAUDE/ADRs (excluding std::...)
# must exist as a pub item in src/<module>.rs, or in the source of a crate that the
# module re-exports with `pub use <crate>::*`. Also prints measured dep counts that
# the docs quote (README/ADR 0002/ADR 0006). Exit 1 on symbol drift.
cd "$(dirname "$0")/.."
item='pub (const )?(fn|struct|enum|mod|type|trait)'
# Source directory of a dependency, from cargo metadata.
src_of() {
  cargo metadata --format-version 1 --all-features 2>/dev/null \
    | jq -r --arg c "$1" '.packages[] | select(.name == $c) | .manifest_path' \
    | head -n1 | xargs -r dirname
}
out=$(for f in README.md CLAUDE.md docs/adr/*.md; do
  for m in path env fs sh http print time archive re hash script; do
    glob=$(grep -oP '^pub use \K\w+(?=::\*;)' "src/$m.rs" | head -n1)
    dir=$([ -n "$glob" ] && src_of "$glob")
    grep -noP "(?<![:\w])$m::\K[a-z_][a-z0-9_]*" "$f" | while IFS=: read -r line name; do
      grep -qE "$item $name\b|pub use .*\b$name\b" "src/$m.rs" \
        || { [ -n "$dir" ] && grep -rqE "$item $name\b" "$dir/src"; } \
        || echo "DRIFT $f:$line $m::$name"
    done
  done
done | sort -u)
echo "symbol drift: $(printf '%s' "$out" | grep -c DRIFT)"; [ -n "$out" ] && echo "$out"
n(){ cargo tree -e normal --prefix none "$@" 2>/dev/null | sed 's/ (\*)//' | sort -u | grep -vc '^smop '; }
echo "crates: sh,fs,path=$(n --no-default-features --features sh,fs,path) (docs: 12)  full=$(n) (docs: ~140)  http=$(n --no-default-features --features http) (docs: ~40)"
[ -z "$out" ]
