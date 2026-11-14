# ADR 0021: Prompts Without a Terminal, and CI Annotations

## Status

Accepted (0.5 cycle). Issue [madmax983/smop#51](https://github.com/madmax983/smop/issues/51).

## Context

- dialoguer checks only the terminal that it draws on (stderr). If stdin
  is not a terminal, console reads keys from `/dev/tty`. Because of this,
  `my-script < /dev/null` waits for a person.
- In CI, `confirm_default` returned an error. When nobody can answer, the
  default is a correct answer.
- Harvest's xtask writes `::error::` by hand 24 times in 12 files. The
  xtask does not escape the text. A line break cuts the message.
- `success!` and `warn!` write to stdout. Status text on stdout mixes
  with data that a script writes there.

## Decision

- A prompt asks only if stdin and stderr are terminals. If not:
  - `confirm_default` and `prompt_default` return the default at once.
  - `confirm`, `prompt`, `select`, `multiselect` and `password` return an
    error at once. The error names the prompt and the stream that is not
    a terminal: `no terminal for the prompt "Deploy?": stdin is not a
    terminal`.
- `select` and `multiselect` take `&[T: Display]` and return indexes, as
  dialoguer does. An empty list is an error before the terminal check.
  `password` uses the dialoguer default: if the input is empty, the
  prompt asks again.
- `info!` (`ℹ`) and `step!` (`→`, bold text) write to stderr.
  `success!` and `warn!` stay on stdout. A move is a breaking change.
- `ci_error`, `ci_warning` and `group` write to stderr. The runner reads
  workflow commands from stdout and stderr with the same code
  (`ScriptHandler.cs`, `OutputManager.cs`).
  - Only `GITHUB_ACTIONS=true` (the value that GitHub sets) gives the
    `::` form. Other values give plain lines.
  - The `::` text escapes `%`, CR and LF as `%25`, `%0D` and `%0A`
    (`ActionCommand.cs`). Then a message stays one command. It cannot
    start a new command.
  - Plain lines are the `error!`, `warn!` and `step!` lines.
    `group` writes no end line outside GitHub Actions.
  - `group(name, || ..)` returns the value of the closure. A drop guard
    writes `::endgroup::`, also after `?` or a panic.
- Lines on stderr use the colour choice of stderr (`for_stderr`).
- The status lines and annotations hide the live spinners and progress
  bars while they print (`suspend`). Else a redraw overwrites the line.
- dialoguer does not show the cursor after a list prompt fails (Ctrl-C).
  smop shows it.
- `password` reads in line mode, so a caught Ctrl-C does not stop the
  read. After each line, it checks for an interrupt from `script::guard`.

## Options considered

- A `GroupGuard` that the caller keeps: `let _ = print::group(..)` drops
  it at once and the group is empty. A closure cannot have this error.
  `script::timed` also takes a closure.
- Macros for annotations: two more names at the crate root. A function
  with `impl Display` takes `format_args!`.
- Check only stderr, as dialoguer does: the hang in Context stays.
- `prompt_default` fails without a terminal: one rule for the two
  `*_default` prompts is easier to remember.
- Annotation properties (`file=`, `line=`): no user needs them now. A
  later release can add them without a breaking change.
- An opt-in to read `/dev/tty` when stdin is a pipe: no user needs it now.

## Consequences

- `confirm_default` and `prompt_default` in CI return the default.
  `cargo-semver-checks` does not find this change. A changelog fragment
  records it.
- A script that reads data on stdin (`cat list | tool`) cannot prompt.
  Give the answer with a flag or an environment variable.
- With stderr redirected (`tool 2>log`), a person cannot see a prompt.
  `confirm_default` and `prompt_default` then return the default.
- The runner reads stdout and stderr through two pipes. A body that
  writes to stdout can show a line out of order next to a group line.
- The tests that write annotations run in a child process. In CI, an
  in-process `::error::` line makes a real annotation on the run.
- Tests use `script` to give the child a pty. With stdin null, the
  prompts must not wait. Without the stdin check, this test hangs. On
  Linux, a second test types keys, so the prompts must ask.
- GitHub does not nest groups. Do not call `group` in a group.
