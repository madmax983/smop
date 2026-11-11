# ADR 0010: `sh` Process I/O

## Status

Accepted (0.4.0).

## Context

Harvest's xtask writes process I/O by hand in many files: capture with
status, `tee`, `>`, `>>`, `2>&1`, `>&2`, stdin from text, quiet runs and a
timeout. A long command shows no output until it ends. A child that writes
more than a pipe holds to both streams blocks when the parent reads only
one stream.

## Decision

- One engine (`src/sh/stdio.rs`) runs all run methods of
  `CommandBuilder`: `run`, `status`, `success`, `output`, `output_lines`,
  `capture` and `output_raw`.
- Each stream gets a set of sinks: the terminal, a buffer (all, or the last
  4 KiB) and files. A stream with only one plain sink goes to the child as a
  `Stdio` (inherit, null or file). Other streams go through a pipe. A reader
  thread for each pipe copies each chunk to all sinks. Thus the child never
  blocks on a full pipe.
- A thread writes `stdin`. smop ignores its write errors, as a shell does
  when the child closes stdin early.
- `2>&1` and `>&2` give one pipe or file to both streams of the child
  (`io::pipe` and `try_clone`). Thus the OS keeps the order of the lines.
  The last merge call wins, so the two merges can not make a cycle.
- `timeout` polls `try_wait` (1 ms, then up to 20 ms). At the deadline it
  kills the child and reaps it. The error has code 124, as GNU `timeout`
  gives. It also has the output that smop captured before.
- smop kills only the direct child. A grandchild can keep a pipe open. With
  a timeout, smop waits for the readers until the deadline plus 200 ms, and
  then returns. The reader thread continues alone until the pipe closes.
  Without a timeout, smop waits for the end of the stream, as `$(...)`
  does. The `spawn_guarded` work adds process group kill.
- `capture` returns `sh::Captured`: the status, the shell-style `code()`
  (`u8`) and stdout and stderr as text, decoded lossily. A non-zero exit is
  not an error. `output_raw` gives the bytes.
- `quiet` keeps output off the terminal. A quiet `run` keeps the last
  4 KiB of stderr for its error. `echo` still prints the command line.
- `sh::CommandError` gets `stdout()` and `timed_out()`. The error of a
  capture method holds the captured stdout and stderr.
- `spawn` gives an error for an I/O option other than `quiet`. It can not
  honour them, and to ignore them silently is worse.
- The engine uses only `std`: no new crate and no `unsafe` code. `io::pipe`
  needs Rust 1.87. The MSRV is 1.89.

## Consequences

- Each run method costs up to three threads. A script runs few commands at
  one time, so this cost is small.
- The tee to the terminal writes each chunk and flushes. The child's own
  buffering decides how fast lines show.
- On Windows, `kill` stops only the direct child, as on Unix.
- The tests that look at the terminal of the test process run a case in a
  new test process (`tests/sh_io.rs`, `child_entry`).
- A Verus spec does not fit this module: it is threads and OS I/O. Proptests
  cover the tail buffer. The plan from options to sinks has unit tests.
