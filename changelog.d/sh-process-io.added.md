- **`sh` process I/O**. New `CommandBuilder` methods:
  - `capture() -> Captured`: the status, the shell-style `code() -> u8`, and
    stdout and stderr as text (decoded lossily). A non-zero exit is not an
    error.
  - `output_lines()`: the lines of stdout (`\n` or `\r\n`).
  - `tee()`: shows output live while smop captures it.
  - `log_stdout(path)` and `log_stderr(path)`: copy a stream to a file.
  - `stdout_file(path)` (`>`), `stdout_append(path)` (`>>`),
    `stderr_to_stdout()` (`2>&1`) and `stdout_to_stderr()` (`>&2`).
  - `stdin(text or bytes)`, `quiet()` and `timeout(Duration)`.

  Threads read stdout and stderr while smop waits, so a child that writes
  much to both streams does not block. A timeout kills the child, reaps it,
  and gives an error with code 124 that holds the output captured before.
  `sh::CommandError` adds `stdout()` and `timed_out()`. A quiet `run` puts
  the last 4 KiB of stderr in its error. `spawn()` gives an error for an
  I/O option other than `quiet`. See ADR 0010.
