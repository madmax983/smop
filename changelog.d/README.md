# Changelog fragments

Each change that users can see gets one file here. Two PRs do not edit the
same file, so they do not conflict.

## Add a fragment

1. Make a file `changelog.d/<name>.<section>.md`.
   - `<name>`: lowercase words with hyphens, for example `fs-temp-dir-type`.
   - `<section>`: `breaking`, `added`, `changed`, `deprecated`, `removed`,
     `fixed` or `security`.
2. Write one or more Markdown list items (`- `), as in `CHANGELOG.md`.
3. For a breaking change, write a migration note.
4. Run `cargo xtask changelog check`. CI runs it too.

`cargo xtask changelog preview` shows the next release notes.

## Release

1. `version` in `Cargo.toml` and `smop-macros/Cargo.toml` is the release,
   and `smop`'s `smop-macros` dependency pins it with `=`. The README's
   `smop = "x.y"` lines name it.
2. `cargo xtask changelog release 0.4.0` writes the fragments into
   `CHANGELOG.md` with today's date and a `[0.4.0]:` compare link, and
   removes them. Add a short intro by hand, with the breaks that the
   compiler does not show. Commit both. If the publish is on a later day,
   change the date in the heading.
3. On trunk, after the release PR merges: `cargo publish --workspace
   --dry-run`, then `cargo publish --workspace` (Cargo 1.90 or later). It
   publishes `smop-macros` before `smop`.
4. `cargo xtask tag-release 0.4.0`. It tags `v0.4.0` on the commit in
   the published crate's `.cargo_vcs_info.json`.
5. `git push origin v0.4.0`.
6. CI's semver job compares with the latest smop on crates.io, so from now
   on it compares with 0.4.0. Nothing to change.

`cargo xtask changelog check` fails if a released version has no link line.
