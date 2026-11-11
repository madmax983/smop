# ADR 0010: `script` Module and the Interrupt Guard

## Status

Accepted (0.4.0).

## Context

A script must clean up after Ctrl-C, SIGTERM or SIGHUP. Then it must stop
in a way that a calling shell understands. Harvest's xtask does this with
its own `interrupt.rs`. That file declares C's `signal` and calls it in
`unsafe` blocks. smop is `#![deny(unsafe_code)]`.

We must select one of these:

- `signal-hook`. `flag::register` sets a flag. `register_conditional_default`
  runs the default action when a flag is `true`. Its handlers stay for the
  life of the process. When the last action is removed, the signal is
  ignored. It does not get its default action again.
- One reviewed `#[allow(unsafe_code)]` block that calls `sigaction` (Unix)
  and `SetConsoleCtrlHandler` (Windows) through our own `extern` items.
- `ctrlc`. It installs one handler for each process and never removes it.
  A second install fails with `MultipleHandlers`. It does not fit.

## Decision

- Use `signal-hook` 0.4 with `default-features = false`. smop has no
  `unsafe` code.
- The guard is behind the `script-signals` feature. `script` has no
  dependencies. `script` implies `sh`, which also has none.
- The handlers install one time, at the first `guard()`. Each signal gets
  three actions, in this order:
  1. Unix: `register_conditional_default(signal, DEFAULT)`. Windows:
     `register_conditional_shutdown(signal, 130, DEFAULT)`, because the
     Windows default action exits with 3.
  2. `register_usize(signal, SIGNAL, signal)` keeps the signal number.
  3. `register(signal, DEFAULT)` sets the flag for the next signal.
- `DEFAULT` is `false` only while a guard lives and no signal came. The
  result is:
  - Without a live guard, a signal gets its default action.
  - With a live guard, the first signal sets `SIGNAL`. The second signal
    gets its default action.
- Unix signals: SIGINT, SIGTERM and SIGHUP. Windows: SIGINT (Ctrl-C) and
  SIGBREAK (Ctrl-Break) from the C runtime.
- After an interrupt, `sh` starts no new command. It returns an error that
  holds `script::Interrupted`. `CommandBuilder::cleanup()` turns off this
  check. `script::defer` uses it.
- After an interrupt, the script function of `script::main` returns. Its
  `defer` values drop, so their commands run. Then, on Unix, `main` calls
  `emulate_default_handler`, so the process stops by the signal. This is
  also true when the function returns `Ok`. bash stops a `for` loop only
  when its child stops by SIGINT. An exit with 130 lets the loop continue.
  On Windows, `main` exits with 130.
- `SIGNAL` stays set after the guard drops, so that `main` can see it.

## Consequences

- `script-signals` adds 4 crates on Linux: `signal-hook`,
  `signal-hook-registry`, `libc` and `errno`. CI checks that
  `cargo tree --no-default-features --features script` has no signal crate.
- A guard does not stop Rust code that does not call `sh` or `retry`. Such
  a loop must call `script::interrupted()`.
- The handlers stay for the life of the process. A signal that was ignored
  at start (for example SIGHUP under `nohup`) is caught while a guard
  lives. After the last guard drops, it gets its default action, and a
  child no longer inherits the ignore. To keep the ignore, smop must read
  the old disposition with `sigaction`, and that needs `unsafe` code.
- On Windows, Ctrl-C without a live guard exits with 130, not with
  `STATUS_CONTROL_C_EXIT`.
- On Windows, the C runtime can lose a signal that comes while it installs
  a handler. This is a limit of `signal-hook` on Windows.
- Tests send real signals to a child process. On Windows, the test sends
  itself SIGINT with `raise`, because a console Ctrl-C event cannot be sent
  to one child without `unsafe` code.
