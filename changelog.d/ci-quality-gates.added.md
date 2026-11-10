- CI quality gates (#32):
  - a line-coverage gate: `cargo llvm-cov --workspace --all-features
    --fail-under-lines 85`, the low end of the project's 85–90% standard
    (91.5% today)
  - `cargo-semver-checks` against the latest smop on crates.io
