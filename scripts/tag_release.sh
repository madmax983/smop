#!/usr/bin/env bash
# Tags published versions: `v<version>` on the commit in the
# .cargo_vcs_info.json of the crate on crates.io. A tag that exists must
# point at that commit. Needs curl, tar and jq.
# Run: bash scripts/tag_release.sh 0.4.0 [more versions]
# Then: git push origin v0.4.0
set -euo pipefail
cd "$(dirname "$0")/.."
[ $# -gt 0 ] || { echo "usage: $0 <version>..." >&2; exit 2; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

for version in "$@"; do
  crate="$tmp/smop-$version.crate"
  curl -sSfL "https://static.crates.io/crates/smop/smop-$version.crate" -o "$crate"
  info=$(tar -xzOf "$crate" "smop-$version/.cargo_vcs_info.json")
  sha=$(jq -r '.git.sha1 // ""' <<<"$info")
  [[ "$sha" =~ ^[0-9a-f]{40}$ ]] || { echo "$version: no git commit in the crate" >&2; exit 1; }
  [ "$(jq -r '.git.dirty // false' <<<"$info")" = false ] \
    || { echo "$version: published from a dirty tree" >&2; exit 1; }
  git cat-file -e "$sha^{commit}" \
    || { echo "$version: commit $sha is not in this clone" >&2; exit 1; }
  git show "$sha:Cargo.toml" | grep -x "version = \"$version\"" >/dev/null \
    || { echo "$version: Cargo.toml at $sha has another version" >&2; exit 1; }

  tag="v$version"
  if old=$(git rev-parse -q --verify "refs/tags/$tag^{commit}"); then
    [ "$old" = "$sha" ] || { echo "$tag: points at $old, not $sha" >&2; exit 1; }
    echo "$tag: ok ($sha)"
  else
    git tag -a "$tag" "$sha" -m "smop $version"
    echo "$tag: made ($sha)"
  fi
done
