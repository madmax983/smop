- `Cargo.toml` declares a workspace, and the lint table moved to
  `[workspace.lints]`, so `smop-macros` (ADR 0007) can join and share it.
  CI passes `--workspace`, and its per-feature loops read the feature list
  from `cargo metadata` (`scripts/ci-features.sh`) instead of a hand-written
  copy.
