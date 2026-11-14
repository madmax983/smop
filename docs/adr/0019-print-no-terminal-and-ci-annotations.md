# ADR 0019: Prompts Without a Terminal, and CI Annotations

## Status

Accepted (0.5 cycle). Issue [madmax983/smop#51](https://github.com/madmax983/smop/issues/51).

## Context

- dialoguer checks only the terminal that it draws on (stderr). When
  stdin is not a terminal, console reads keys from `/dev/tty`. Thus
  `script < /dev/null` from a shell waits for a person.
- A script in CI that calls `confirm_default` failed, but a default is a
  correct answer when nobody can answer.
- Harvest's xtask writes `::error::` by hand 24 times in 12 files. The
  text has no escapes, so a message with a line break is cut.
- `success!` and `warn!` write to stdout. Progress text on stdout mixes
  with data that a script writes there.

## Decision

- A prompt asks only when stdin and stderr are both terminals. Else:
  - `confirm_default` and `prompt_default` return the default at once.
  - `confirm`, `prompt`, `select`, `multiselect` and `password` return an
    error at once. It names the prompt and the stream that is not a
    terminal: `no terminal for the prompt "Deploy?": stdin is not a
    terminal; ...`.
- `select` and `multiselect` take `&[T: Display]` and return indexes, as
  dialoguer does. An empty list is an error before the terminal check.
  `password` uses dialoguer's default: an empty input asks again.
- `info!` (`ℹ`) and `step!` (`→`, bold text) write to stderr.
- `ci_error`, `ci_warning` and `group` write to stderr. The runner reads
  workflow commands from stdout and stderr with the same code
  (`ScriptHandler.cs`, `OutputManager.cs`).
  - Only `GITHUB_ACTIONS=true` (the value that GitHub sets) gives the
    `::` form. Other values give plain lines.
  - Data escapes `%`, CR and LF as `%25`, `%0D` and `%0A`
    (`ActionCommand.cs`). Thus a message stays one command and cannot
    start a new one.
  - Plain lines are the `error!`, `warn!` and `step!` lines.
    `group` writes no end line outside GitHub Actions.
  - `group(name, || ..)` returns the value of the closure. A drop guard
    writes `::endgroup::`, also after `?` or a panic.

## Options considered

- A `GroupGuard` that the caller keeps: `let _ = print::group(..)` drops
  it at once and the group is empty. A closure cannot have this error,
  and `script::timed` uses the same form.
- Macros for annotations: two more names at the crate root. A function
  with `impl Display` takes `format_args!`.
- Only check stderr, as dialoguer does: the hang in Context stays.
- `prompt_default` fails without a terminal: one rule for all `*_default`
  prompts is easier to remember.
- Annotation properties (`file=`, `line=`): no user needs them now. They
  can be added with no breaking change.

## Consequences

- `confirm_default` and `prompt_default` in CI return the default.
  `cargo-semver-checks` does not see this; a `changed` fragment tells it.
- The tests that write annotations run in a child process. In CI, an
  in-process `::error::` line makes a real annotation on the run.
- A test on Linux uses util-linux `script` to give the child a pty on
  stderr with stdin null. It hangs without the stdin check.
- GitHub does not nest groups. Do not call `group` in a group.
