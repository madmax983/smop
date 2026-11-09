#!/usr/bin/env bash
# Clean-room run of the documented first-run journey, with malicious literalism:
#   1. cargo new (the only step the README assumes you know)
#   2. paste the README "Installation" block into Cargo.toml
#   3. paste the README's FIRST rust block (the front-door snippet) into src/main.rs, verbatim
#   4. cargo run
# Logs every command with exit code and wall seconds, then a summary.
# Dependency source: crates.io by default (what a real newcomer gets);
# SMOP_PATH=/path rewrites the dependency to a checkout for pre-release runs.
# Run in a pristine container:  docker build -f scripts/dx/Dockerfile -t smop-cleanroom . && docker run --rm smop-cleanroom
set -u
readme="${README:-$(cd "$(dirname "$0")/../.." && pwd)/README.md}"
work="$(mktemp -d)"; cd "$work" || exit 2
steps=0; fails=0; t0=$(date +%s); log=()
step(){ # step "label" cmd...
  local label="$1"; shift; steps=$((steps+1)); local s=$(date +%s)
  local out; out=$("$@" 2>&1); local rc=$?
  log+=("$(printf '%2d  rc=%-3d %4ss  %s' "$steps" "$rc" "$(( $(date +%s)-s ))" "$label")")
  [ $rc -ne 0 ] && { fails=$((fails+1)); log+=("      first error: $(echo "$out" | grep -m1 -E '^error' )"); echo "$out" | grep -E -A3 -m2 '^error' | sed 's/^/      | /' >> "$work/errors.txt"; }
  return $rc
}
step "cargo new hello" cargo new -q hello || exit 2
cd hello
# README Installation: first ```toml block
python3 -I - "$readme" <<'P' >> Cargo.toml
import re,sys
t=open(sys.argv[1]).read()
b=re.search(r"```toml\n(.*?)```",t,re.S).group(1)
print(b.replace("[dependencies]\n","",1).rstrip() if False else b.split("[dependencies]\n",1)[1].rstrip())
P
[ -n "${SMOP_PATH:-}" ] && sed -i "s#^smop = \"[^\"]*\"#smop = { path = \"$SMOP_PATH\" }#" Cargo.toml
step "paste README Installation block into Cargo.toml" grep -q '^smop' Cargo.toml
# first rust block, verbatim
python3 -I - "$readme" <<'P' > src/main.rs
import re,sys
print(re.search(r"```rust\n(.*?)```",open(sys.argv[1]).read(),re.S).group(1),end="")
P
step "paste README first rust block into src/main.rs (verbatim)" test -s src/main.rs
step "cargo run" cargo run -q
rc=$?
wall=$(( $(date +%s)-t0 ))
echo "== clean-room log =="; printf '%s\n' "${log[@]}"
[ -f "$work/errors.txt" ] && { echo "== errors =="; cat "$work/errors.txt"; }
echo "== summary: steps=$steps failed_steps=$fails wall_seconds=$wall first_success=$([ $rc -eq 0 ] && echo yes || echo NO) =="
exit $rc
