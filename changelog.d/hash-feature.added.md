- `hash` feature (in `full`): `hash::sha256(bytes)`, `hash::sha256_file(path)`
  and `hash::verify_sha256_file(path, expected)` give SHA-256 as lowercase
  hex. `sha256_file` reads through one 64 KiB buffer, so memory use does not
  change with the file size. Errors name the path. A mismatch error shows
  both digests. Adds `sha2` (8 crates; 9 on aarch64 Linux and Apple).
