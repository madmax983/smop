# ADR 0008: One Containment Policy for Archive Extraction

## Status

Accepted (0.4.0). Issue [madmax983/smop#35](https://github.com/madmax983/smop/issues/35).

## Context

Up to 0.3, the three extract functions did not agree, and they failed silently:

- `extract_zip` skipped an entry that went out of `dest`. It wrote
  `/abs.txt` to `dest/abs.txt`. It kept `a/../b.txt` as `dest/b.txt`.
- `extract_tar` and `extract_tar_gz` skipped each entry with a `..` step,
  also `a/../b.txt`. They wrote `/abs.txt` to `dest/abs.txt`.
- All three returned `Ok`.
- A tar link `link -> ..` stayed on disk after the error that it caused.
- `extract_zip` wrote through a link that was in `dest` before the
  extraction.

## Decision

All formats use one module, `src/archive/contain.rs`, with these rules:

| Entry | Result |
|---|---|
| Absolute path (`/a`, `C:\a`, `\\server\a`) | Error |
| `a/../b.txt` | Extracts to `dest/b.txt` |
| `../b.txt`, `a/../../b.txt` | Error |
| `.`, `./` directory | No operation (it is `dest`) |
| `.` file or link | Error |
| Link to a path in `dest`, also with `..` | Extracts as a link |
| Link to a path outside `dest`, or to an absolute path | Error. No link stays on disk. |
| Tar hard link to a file in `dest` | Extracts |
| Tar hard link to a path outside `dest`, or to no file | Error |
| A name with `:` (Windows only) | Error |

How it works:

1. `normalize` removes `.` and `..` steps by name. It does not read the disk.
2. `resolve` finds the real location of a path. It follows each link that is
   on disk, also links that were in `dest` before the extraction. A link on
   disk with an absolute target counts as outside `dest`.
3. A file or link entry follows links in its parent directories only. A
   file or link at the last name is removed, not written through.
4. A later link can change where an earlier link goes (`b -> p/../x`, then
   `p -> .`). Thus, after the entries, and also after an error, each link
   that the extraction made is checked again. A link that goes out of `dest`
   is removed, and the extraction returns an error.

Tar files and directories still go through `tar::Entry::unpack`, so file
modes and times stay as before. Directory entries are applied last, deepest
first, as `tar::Archive::unpack` does.

ZIP link entries now become links, as in tar. Before, they became files
that held the target path.

The error names the entry and the archive:
`Unsafe entry "../evil.txt" in archive a.zip: the path goes outside the destination`.

## Consequences

- Extraction errors where it returned `Ok` before. This is a breaking
  change in behaviour only. `cargo-semver-checks` does not see it.
- On error, `dest` can hold the entries before the unsafe entry.
- A proptest (`tests/archive_safety.rs`) checks the containment property
  over generated entries, for all three formats.

## Alternatives

- **Keep `enclosed_name` and `unpack_in`, and error on a skipped entry.** The
  two crates use different rules, and neither checks link chains.
- **Strip a leading `/`, as GNU tar does.** It changes the entry silently,
  and 0.4 removes silent changes.
- **Reject each `..` in a link target.** Real archives use `../lib/x.so`
  links.
- **Check all names before the first write.** A tar.gz stream must then be
  read two times, and link safety depends on the disk anyway.
- **Use `cap-std`.** It is a large dependency for one module.

## Not in scope

- Another process that changes `dest` during the extraction.
- Windows device names such as `CON`.
