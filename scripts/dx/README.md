# DX harness

Reproducible measurements of the first-run journey (see the Onramp findings in the PR that added this).

| Script | What it measures |
|---|---|
| `clean_room.sh` | Follows the README verbatim from `cargo new` to `cargo run`: steps, rc, seconds, first error. Exit 0 = first success. |
| `snippets.sh` | Compiles every ```` ```rust ```` block in README.md/CLAUDE.md against this checkout. `FAIL` = defect; `CTX` = fragment naming user-side types (illustrative, tracked but not gated). |

Pristine container: `docker build -f scripts/dx/Dockerfile -t smop-cleanroom . && docker run --rm smop-cleanroom`
Pre-release run against a checkout: `SMOP_PATH=$PWD scripts/dx/clean_room.sh`
