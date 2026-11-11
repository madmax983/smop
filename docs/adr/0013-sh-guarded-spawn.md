# ADR 0013: `sh::spawn_guarded` and the Process Group Kill

## Status

Accepted (0.4.0).

## Context

A script often starts a server, waits until it answers, runs a step, and
then stops the server. Harvest's `dev/run_standalone_chapter.rs` does this
by hand. It starts the server under `setsid`, polls with a `curl` loop,
and stops the group with `kill -INT/-TERM -- -PID` (issue #45).

To kill only the direct child is not sufficient. A server that `sh -c`,
`npm` or `cargo run` starts runs on, and it keeps captured pipes open.
`sleep 30` survived after its `sh` was killed.

std has no stable call that kills a process group
(`Child::kill_process_group` is unstable). smop is
`#![deny(unsafe_code)]`. CI fails when the `sh` feature adds a crate.

We must select one of these:

- `rustix`. Its `kill_process_group` is safe. It is in the tree through
  `tempfile` when `fs` is on, but not with `sh` alone. It breaks the CI
  gate "`sh` adds no crate".
- `libc::killpg` in a reviewed `unsafe` block. It needs `libc` and an
  exception to `deny(unsafe_code)`.
- The kill tool of the platform. Unix: the `kill` builtin of `/bin/sh`.
  Windows: `taskkill /T /F`. No crate and no `unsafe` code.
- Windows Job Object. It needs `windows-sys` and `unsafe` code, or a
  wrapper crate.

## Decision

- Use the kill tool of the platform. `src/sh/group.rs` has all of it.
  - Unix: `/bin/sh -c 'kill -s KILL -- "-$1"' sh PGID`. `kill` is a POSIX
    builtin of each shell. `sh::shell` uses the same full path. Thus a
    `PATH` from `.env()` does not matter.
  - Windows: `taskkill /T /F /PID PID`.
- `spawn_guarded` puts the child in a new process group:
  `CommandExt::process_group(0)` on Unix, `CREATE_NEW_PROCESS_GROUP` on
  Windows. Thus the group id is the process id of the child.
- `ChildGuard` drop kills the group, then kills the child, then reaps it.
  The kill comes before the reap. Until the reap, the child holds its id,
  so no other group can get that id. The guard offers no `try_wait`, which
  would reap early.
- `ChildGuard::wait` and `wait_with_output` kill nothing. `kill` returns
  the errors that drop ignores.
- A timeout of a guarded child kills the group, not only the child.
- Drop ignores all errors. It can run while a panic unwinds.
- `sh::wait_until(timeout, probe)` calls a `bool` probe with a pause of
  10 ms that doubles up to 100 ms. It never sleeps past the end of the
  timeout. It stops with `script::Interrupted` when a guard caught an
  interrupt.

## Consequences

- `sh` still adds no crate and no `unsafe` code.
- Each drop of a guard starts one more process (about 1 ms).
- A guarded child is not in the foreground group of the terminal. Ctrl-C
  does not reach it. Without `script::guard`, Ctrl-C stops the script with
  no drop, and the group runs on. The docs tell the user to use
  `script::main` with `script::guard`.
- A process that leaves the group (`setsid`, a daemon) does not stop.
- On Windows, `taskkill /T` finds the tree from parent process ids. It
  does not find a process whose parent ended before the kill. A Job Object
  closes that gap. A later feature can add one without an API change.
- `panic = "abort"` and `std::process::exit` skip drop.
- Tests that check grandchildren are `#[cfg(unix)]`. They read the state
  of a process with `ps`, because a killed grandchild can stay a zombie in
  a container with no reaper.
