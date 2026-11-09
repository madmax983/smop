#!/usr/bin/env bash
# Prints smop's features, one per line, without `default` and `full`.
#
# CI's per-feature test and clippy loops read the list from here, so a new
# feature in Cargo.toml is tested and linted on its own without editing the
# workflow. It fails instead of printing nothing, so a broken lookup can't
# silently skip every per-feature check.
set -euo pipefail
cd "$(dirname "$0")/.."

features=$(cargo metadata --no-deps --format-version 1 \
  | jq -r '.packages[] | select(.name == "smop") | .features | keys[]
           | select(. != "default" and . != "full")' \
  | tr -d '\r')

if [ -z "$features" ]; then
  echo "ci-features.sh: no features found for smop in cargo metadata" >&2
  exit 1
fi

printf '%s\n' "$features"
