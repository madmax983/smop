- **`archive::extract_zip`, `extract_tar` and `extract_tar_gz` return an
  error for an unsafe entry** (#35), and the error names it. Before, they
  skipped it and returned `Ok`. One policy for all formats (ADR 0008):
  - An absolute path (`/a.txt`) is an error. Before, it extracted to
    `dest/a.txt`.
  - `a/../b.txt` extracts to `dest/b.txt`. Before, tar skipped it.
  - `../b.txt` is an error.
  - A link to a path outside `dest`, or to an absolute path, is an error,
    and the link does not stay on disk. The target must stay in `dest` also
    by name: `l/../../x` is an error even if `l` is a link to `a/b`.
  - No write follows a link out of `dest`. This includes links that were in
    `dest` before the extraction. A file entry replaces a link at its path.
  - On Windows, a name with `:` is an error. Before, `extract_zip` skipped
    it. A device name such as `CON` now extracts as a normal file.
  - ZIP link entries become links. Before, they became files that held the
    target path. On Windows, to make a link can need Developer Mode.
  - A tar hard link to a link becomes a copy of the link. A tar hard link to
    itself does nothing. Before, it gave an error.

  Migration: for an archive from an unknown source, handle the error. If
  `dest` has links with absolute targets, make them relative or extract to
  a new directory. To extract absolute entries, use the `zip` or `tar`
  crate.
