# ADR 0012: `sh` Process I/O

## Status

Accepted (0.4.0).

## Context

Harvest's xtask writes process I/O by hand in many files: capture with
status, `tee`, `>`, `>>`, `2>&1`, `>&2`, stdin from text, quiet runs and a
timeout. A long command shows no output until it ends. A child can write
more to each stream than a pipe holds. If the parent reads only one stream,
the child stops.

## Decision

- One engine (`src/sh/stdio.rs`) runs all run methods of
  `CommandBuilder`: `run`, `status`, `success`, `output`, `output_lines`,
  `capture` and `output_raw`. `spawn` and `spawn_guarded` use it too
  (issue #45): the engine starts the child and its threads, and
  `ChildProcess::wait` finishes later.
- Each stream gets a set of sinks: the terminal, a buffer (all, or the last
  4 KiB) and files. A stream with only one plain sink goes to the child as a
  `Stdio` (inherit, null or file). Other streams go through a pipe. A reader
  thread for each pipe copies each chunk to all sinks. Thus the child never
  stops on a full pipe.
- smop opens all files before the child starts. If a file does not open,
  the child does not start.
- A thread writes `stdin`. smop ignores its write errors, as a shell does
  when the child closes stdin early. Rust programs ignore `SIGPIPE`. A
  program that sets the default action for `SIGPIPE` can stop on such a
  write. The docs tell the user. A fix needs `unsafe` code.
- `2>&1` and `>&2` give one pipe or file to both streams of the child
  (`io::pipe` and `try_clone`). Thus the OS keeps the order of the lines.
  The last merge call replaces the other. Thus the two merges can not make
  a cycle.
- A log of a stream that a merge sends away is an error, not a silent
  no-op.
- `timeout` polls `try_wait` (1 ms, then up to 20 ms). At the deadline it
  kills the child and reaps it. The error has code 124, as GNU `timeout`
  gives. It also has the output that smop captured before.
- The child can end just before the kill. Then its own status counts. On
  Unix, only a status of `SIGKILL` is a timeout.
- The timeout also applies to the output. smop kills only the direct
  child. A grandchild can keep a pipe open. smop waits for such a pipe
  until the deadline plus 200 ms. Then it gives the timeout error. The
  reader thread continues alone until the pipe closes. Without a timeout,
  smop waits for the end of the stream, as `$(...)` does. A timeout of a
  `spawn_guarded` child kills its process group (ADR 0013).
- `output` and `output_lines` wait for the end of stdout. They wait for
  stderr only 200 ms after the child ends, as `$(...)` does. Thus a
  background process that keeps stderr open (for example an `ssh` control
  master) does not stop them. `capture` returns stderr, so it waits for all
  of it.
- A timeout too large for `Instant` is the same as no timeout.
- `capture` returns `sh::Captured`: the status, the shell-style `code()`
  (`u8`) and stdout and stderr as text. smop changes bytes that are not
  UTF-8 to U+FFFD. A non-zero exit is not an error. `output_raw` gives the
  bytes.
- `quiet` keeps output off the terminal. A quiet `run` keeps the last
  4 KiB of stderr for its error. It waits for that stderr only 200 ms after
  the child ends. Thus a command that starts a background process does not
  stop `run`. A quiet `status` keeps no stderr. `echo` still prints the
  command line.
- `sh::CommandError` gets `stdout()` and `timed_out()`. The error of a
  capture method holds the captured stdout and stderr. After `2>&1`, its
  stderr is the end of the merged output.
- The interrupt check of `script::guard` (ADR 0011) runs before the
  engine starts a child. A `cleanup` command with I/O options still runs.
- `spawn` honours each I/O option, as `run` does, but its stdin is empty.
  The timeout starts at the spawn. `keep_output` keeps both streams for
  `ChildProcess::wait_with_output`, which returns `sh::Captured`. Without
  it, the child gets the stdout and stderr of this process (issue #45).
  Before 0.4, `spawn` piped both streams and no one read them, so a child
  stopped after 64 KiB.
- `pipe` gives an error for I/O options and `keep_output`. It can not
  honour them, and to ignore them silently is worse. A pipe checks each
  stage.
- A pipe does not use this engine. Each stage sends stdout to the next
  stage through an OS pipe. The first stage reads the stdin of this
  process. Earlier stages send stderr to the terminal. `output()` keeps
  the stdout and stderr of the last stage. smop waits for the last stage
  first and reads its output while it waits. Then it waits for each other
  stage (issue #43).
- The engine uses only `std`: no new crate and no `unsafe` code. `io::pipe`
  needs Rust 1.87. The MSRV is 1.89.

## Consequences

- Each run method costs up to three threads. A spawned child keeps its
  threads until its streams close. A script runs few commands at
  one time, so this cost is small.
- The tee to the terminal writes each chunk and flushes. The child's own
  buffering controls how fast lines show.
- On Windows, `kill` stops only the direct child, as on Unix.
- The tests that look at the terminal of the test process run a case in a
  new test process (`tests/sh_io.rs`, `child_entry`).
- This module uses threads and OS I/O. Thus a Verus spec does not apply.
  Proptests cover the tail buffer. Unit tests cover the plan from options
  to sinks.
