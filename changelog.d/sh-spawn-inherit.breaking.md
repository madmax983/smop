- **`sh` `spawn()` gives the child the stdout and stderr of this process**
  (#45). Before, it piped both streams and no one read them, so a child
  that wrote more than 64 KiB blocked. The child still gets an empty
  stdin. Migration: add `.keep_output()` and call
  `ChildProcess::wait_with_output()` to get the output as `sh::Captured`,
  or add `.quiet()` to discard it.
- **`sh` `spawn()` honours each I/O option** (#45): `stdin`, `tee`,
  `timeout`, the log files and the redirections. Before, these gave an
  error. The timeout starts at the spawn. A pipe stage with
  `keep_output` gives an error.
- **`ChildProcess` is `#[must_use]`** (#45), because drop does not stop the
  child. Migration: call `.wait()` or `.kill()`, or bind it with
  `let _child = ...`.
