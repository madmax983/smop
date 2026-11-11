- **`sh` `spawn()` gives the child the stdout and stderr of this process**
  (#45). Before, it piped both streams and no one read them, so a child
  that wrote more than 64 KiB blocked. Stdin is still empty, unless
  `stdin()` sets it. Migration: add `.keep_output()` and call
  `ChildProcess::wait_with_output()` to get the output as `sh::Captured`,
  or add `.quiet()` to discard it.
- **`sh` `spawn()` honours each I/O option** (#45): `stdin`, `tee`,
  `timeout`, the log files and the redirections. Before, these gave an
  error. The timeout starts at the spawn. Only `wait()` and
  `wait_with_output()` apply it. A pipe stage with `keep_output` gives an
  error.
- **A quiet `spawn()` keeps the last 4 KiB of stderr** (#45), as a quiet
  `run()` does. A thread reads stderr, and the error of `wait()` holds the
  tail. Before, stderr went to the null device.
- **`ChildProcess` is `#[must_use]`** (#45), because drop does not stop the
  child. Migration: call `.wait()` or `.kill()`, or bind it with
  `let _child = ...`.
