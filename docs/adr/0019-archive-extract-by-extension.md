# ADR 0019: `archive::extract` Selects the Format by Extension

## Status

Accepted (0.5.0). Issue [madmax983/smop#52](https://github.com/madmax983/smop/issues/52).

## Context

A script that gets an archive must select one of three `extract_*`
functions. Most scripts select it from the file name.

## Decision

`archive::extract(archive, dest)` reads the end of the file name and calls
one function:

| End of file name | Function |
|---|---|
| `.zip` | `extract_zip` |
| `.tar` | `extract_tar` |
| `.tar.gz`, `.tgz` | `extract_tar_gz` |

- The letter case is not important.
- Only the file name counts, not the directory names.
- The compare uses the bytes of the name, so a name that is not UTF-8 also
  works.
- The name `.zip` alone is a ZIP archive. The rule is "the name ends with".
- A different end is an error before any I/O. The error names the file and
  lists the extensions. `dest` is not made.
- The format type stays private. A new extension does not change the API.

All the rules of [ADR 0008](0008-safe-archive-extraction.md) apply, because
`extract` only calls the `extract_*` functions. `tests/archive_safety.rs`
runs each containment test, and the proptest, through both entry points.

## Consequences

- A file with the wrong extension gives the error of the selected function,
  for example `Failed to read ZIP archive` for a TAR file named `a.zip`.
- Other formats (`.tar.bz2`, `.tar.xz`, `.gz`) are errors.

## Alternatives

- **Read the first bytes (magic numbers).** TAR has no magic number at
  offset 0, and a `.gz` that is not a TAR is a different thing. The file
  name is clear and the user controls it.
- **A public `Format` enum.** It adds API for a function that has one use.
  It can come later without a break.
- **Use `Path::extension`.** It gives only `gz` for `a.tar.gz`.

## References

- [ADR 0008](0008-safe-archive-extraction.md): containment rules.
- [madmax983/smop#52](https://github.com/madmax983/smop/issues/52)
