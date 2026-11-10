#!/usr/bin/env bash
# Checks smop's public API for types of dependencies that smop does not
# re-export on purpose (see "API boundaries" in CLAUDE.md).
#
# Rustdoc links a type to its crate's docs. This script documents smop
# together with its dependencies, then finds links from smop's pages to a
# dependency that is not on the allow list. The dependency list comes from
# cargo metadata, so a new dependency is checked without a change here.
set -euo pipefail
cd "$(dirname "$0")/.."

# Re-exported on purpose: their major releases are smop major releases.
allowed='^(anyhow|clap|serde|serde_json|chrono)$'

deps=$(cargo metadata --no-deps --format-version 1 \
  | jq -r '.packages[] | select(.name == "smop") | .dependencies[]
           | select(.kind == null) | .name' \
  | tr -d '\r' | grep -vE "$allowed" | sort -u)

if [ -z "$deps" ]; then
  echo "api_boundary.sh: no dependencies found for smop in cargo metadata" >&2
  exit 1
fi

pkgs=()
for dep in $deps; do pkgs+=(-p "$dep"); done

# Pages of removed items stay in target/doc, so start clean.
rm -rf target/doc/smop
cargo doc -p smop "${pkgs[@]}" --no-deps --all-features --quiet

# Local links are ../<crate>/ or ../src/<crate>/; links to docs.rs name the
# crate after the host. A trait page also lists the types that implement the
# trait (for example toml's types on serde's traits). Those lists are not
# smop's API, so the scan stops where they start.
names=$(printf '%s\n' "$deps" | tr '-' '_' | paste -sd '|')
hits=$(find target/doc/smop -name '*.html' -print0 | xargs -0 perl -0777 -ne '
  s/id="(?:foreign-impls|implementors)".*//s;
  my $file = $ARGV =~ s{^target/doc/}{}r;
  print "$file: $1\n" while m{href="([^"]*(?:\.\./|docs\.rs/)(?:src/)?(?:'"$names"')/[^"]*)"}g;
' | sort -u)

if [ -n "$hits" ]; then
  echo "Dependency types in smop's public API:"
  printf '%s\n' "$hits"
  exit 1
fi
echo "api boundary: ok ($(printf '%s\n' "$deps" | wc -l) dependencies checked)"
