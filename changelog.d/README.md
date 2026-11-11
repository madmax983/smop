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
4. Run `python3 scripts/changelog.py check`. CI runs it too.

`python3 scripts/changelog.py preview` shows the next release notes.

## Release

`python3 scripts/changelog.py release 0.4.0` writes the fragments into
`CHANGELOG.md` with today's date and removes them. Commit both.
