# ADR 0017: The Repository's Tooling Is a `cargo xtask` Written with smop

## Status

Accepted (0.5 cycle). Issue [madmax983/smop#84](https://github.com/madmax983/smop/issues/84).

## Context

smop is a library for scripts and `cargo xtask` crates. Its own checks and
release steps were 7 bash scripts and 4 Python files in `scripts/`, about
680 lines. They needed `jq` and `python3` besides cargo, and ran only on
Linux CI. They were the kind of script smop exists to replace:

- `cargo metadata | jq`: `ci-features.sh`, `examples.sh`, `docs_drift.sh`
- `curl | tar | jq` and `git`: `tag_release.sh`
- regex scans over Markdown and rustdoc HTML: `docs_drift.sh`,
  `docs_features.py`, `api_boundary.sh` (an inline Python program)
- the changelog tool and its tests: `changelog.py`, `test_changelog.py`

A gap in smop's API showed up in these scripts only as a script written
in another language. We did not notice the gaps.

## Decision

- An `xtask` workspace member, `publish = false`, holds every task.
  `.cargo/config.toml` has the alias `xtask = "run --quiet --package xtask --"`.
  `cargo xtask --help` lists the tasks. CI runs the same commands as a
  contributor.
- It depends on smop from the checkout, with only the features it uses:
  `sh`, `fs`, `path`, `json`, `re`, `git`, `script`, `macros`, `cli`,
  `http`, `archive`, `time`. `cli` gives the subcommands, `script::main` the
  exit codes, the `serde` derive through `smop::serde` reads `cargo
  metadata`, `cmd!` runs `git`, and `http::download` with
  `archive::extract_tar_gz` replaces `curl | tar`.
- Each task keeps the behaviour and output of its script, so CI logs read
  the same. Tested on the repository, and on planted drift and a planted
  API leak. One change: `snippets` counts a failure with no `src/` errors
  (a linker error, say) as broken, not as "needs context".
- Logic is in plain functions with unit tests (`cargo test --workspace`
  runs them, on all three CI systems). The functions that run cargo, git or
  HTTP are thin wrappers around those.
- `scripts/` is gone. A new check is a task in `xtask/src`, not a script.

## Consequences

- No `jq` or Python in CI or for contributors. The tasks run on Windows and
  macOS too.
- smop dogfoods its README's xtask story. A rough edge in `sh`, `fs`, `re`
  or `git` now costs us first.
- CI builds the xtask, 112 crates, all of them smop dependencies. Jobs
  that compile smop anyway pay a few seconds. Jobs that compiled nothing
  before (docs drift) pay a cold build once; the cargo cache keeps later
  runs short.
- A change that breaks smop also breaks the xtask, so a broken tree can't
  run its own docs checks. The build and test jobs fail first, so the
  docs jobs add no signal then.
- The clean-room Docker image (`xtask/clean-room.Dockerfile`) copies the
  repository and runs `cargo xtask clean-room`. The run itself still uses
  smop from crates.io in a new crate, as a newcomer does.
