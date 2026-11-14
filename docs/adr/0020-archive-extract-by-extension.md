# ADR 0020: `archive::extract` Selects the Format by Extension

## Status

Accepted (0.5 cycle). Issue [madmax983/smop#52](https://github.com/madmax983/smop/issues/52).

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
- `extract` reads only the file name. It does not read the directory names.
- `extract` compares the bytes of the name. Thus a name that is not UTF-8
  also works.
- A file with the name `.zip` is a ZIP archive, because the name ends with
  `.zip`.
- A different end is an error before any I/O. The error names the file and
  lists the extensions. `extract` does not make `dest`.
- The format type stays private. A new extension does not change the API.

All the rules of [ADR 0008](0008-safe-archive-extraction.md) apply, because
`extract` only calls the `extract_*` functions. `tests/archive_safety.rs`
runs each containment test, and the proptest, through both entry points.

## Consequences

- A file with the wrong extension gives the error of the selected function,
  for example `Failed to read ZIP archive` for a TAR file named `a.zip`.
- Other formats (`.tar.bz2`, `.tar.xz`, `.gz`) are errors.

## Alternatives

- **Read the first bytes (magic numbers).** TAR has its magic number
  (`ustar`) at offset 257, and an old V7 TAR has none. The gzip magic number
  does not show that the content is a TAR. The user controls the file name,
  and the file name is clear.
- **A public `Format` enum.** It adds API for a function that has one use.
  We can add it later. This change does not break the API.
- **Use `Path::extension`.** It gives only `gz` for `a.tar.gz`.

## References

- [ADR 0008](0008-safe-archive-extraction.md): containment rules.
- [madmax983/smop#52](https://github.com/madmax983/smop/issues/52)
