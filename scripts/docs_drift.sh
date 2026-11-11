#!/usr/bin/env bash
# Drift scan. Every `module::symbol` named in README/CLAUDE/ADRs (excluding std::...)
# must exist as a pub item in src/<module>.rs. Also prints measured dep counts that
# the docs quote (README/ADR 0002/ADR 0006). Exit 1 on symbol drift.
cd "$(dirname "$0")/.."
out=$(for f in README.md CLAUDE.md docs/adr/*.md; do
  for m in path env fs sh http print time archive re; do
    grep -noP "(?<![:\w])$m::\K[a-z_]+" "$f" | while IFS=: read -r line name; do
      grep -qE "pub (const )?(fn|struct|enum|mod|type) $name\b|pub use .*\b$name\b" "src/$m.rs" \
        || echo "DRIFT $f:$line $m::$name"
    done
  done
done | sort -u)
echo "symbol drift: $(printf '%s' "$out" | grep -c DRIFT)"; [ -n "$out" ] && echo "$out"
n(){ cargo tree -e normal --prefix none "$@" 2>/dev/null | sed 's/ (\*)//' | sort -u | grep -vc '^smop '; }
echo "crates: sh,fs,path=$(n --no-default-features --features sh,fs,path) (docs: 12)  full=$(n) (docs: ~135)  http=$(n --no-default-features --features http) (docs: ~40)"
[ -z "$out" ]
