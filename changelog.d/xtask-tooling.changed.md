- The repository's checks and release steps are `cargo xtask` tasks
  written with smop (ADR 0017), not bash and Python scripts:
  `cargo xtask changelog check|preview|release`, `ci-features`,
  `docs-drift`, `docs-features`, `api-boundary`, `examples`, `snippets`,
  `clean-room` and `tag-release`. CI and contributors need no `jq` or
  Python. `scripts/` is removed.
