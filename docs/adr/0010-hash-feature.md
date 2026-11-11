# ADR 0010: `hash` Feature

## Status

Accepted (0.4.0).

## Context

Scripts that download pinned files must check their digests. Harvest's
xtask runs `sha256sum` for this in three places. Windows does not have
`sha256sum` by default. Also, the script must parse its output.

## Decision

- The `hash` feature adds `smop::hash`. It is part of `full`.
- The functions:
  - `sha256(bytes) -> String`. It cannot fail, so it does not return
    `Result`.
  - `sha256_file(path) -> Result<String>`.
  - `verify_sha256_file(path, expected) -> Result<()>`.
- A digest is 64 lowercase hex characters, as `sha256sum` shows it.
- `sha256_file` reads the file through one 64 KiB buffer. Memory use does
  not change with the file size. It reads again after `Interrupted`.
- An error names the path. A mismatch error shows the path, the expected
  digest and the actual digest.
- `verify_sha256_file` accepts upper-case hex. It does not remove
  whitespace and does not parse a `sha256sum` line. If `expected` is not 64
  hex characters, the error is `Invalid SHA-256 digest "<expected>" for
  <path>: need 64 hex characters`. It quotes at most 80 characters with
  `{:?}`. The function does not read the file. A bad pin is a different
  problem from a bad file.
- A mismatch error shows `expected` as the caller gives it.
- The comparison is not constant-time. A digest of a public file is not a
  secret.
- `verify_sha256_file` checks the file at a path. Another process can change
  the file after the check. The docs tell the user to use a directory that
  only the script can write.
- `sha256_file` gives the same error for a directory on all platforms:
  `Failed to read file: <path> is a directory`.
- smop encodes hex itself. The output type of `sha2` 0.11 has no
  `LowerHex`, so `format!("{:x}", ..)` does not compile.
- `sha2` is a private dependency. No `sha2` type is in the public API, so a
  major release of `sha2` is not a major release of smop.
- smop turns off the default features of `sha2` (`alloc`, `oid`). smop
  does not use them. With `hash` alone, smop does not get `const-oid`.

## Consequences

- A script needs no `sha256sum` and works on all platforms.
- `hash` adds 8 crates: `sha2`, `digest`, `block-buffer`, `crypto-common`,
  `hybrid-array`, `typenum`, `cpufeatures` and `cfg-if`. On some targets
  (for example aarch64 Linux and Apple), `cpufeatures` adds `libc`.
- `sha2` 0.11 needs Rust 1.88. smop's MSRV is 1.89, so smop's MSRV does not
  change.
- Other algorithms (SHA-512, BLAKE3) are not in this release. To add one,
  add a function to `hash`.
