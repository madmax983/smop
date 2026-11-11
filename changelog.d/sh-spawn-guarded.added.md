- `sh::CommandBuilder::spawn_guarded()` returns a `sh::ChildGuard` (#45).
  The child gets its own process group. Drop of the guard kills the group
  and reaps the child, also while a panic unwinds. `wait()` and
  `wait_with_output()` kill nothing. No new crate and no `unsafe` code
  (ADR 0013).
- `sh::CommandBuilder::keep_output()` and
  `sh::ChildProcess::wait_with_output()`, which returns `sh::Captured`
  (#45).
- `sh::ChildProcess::id()` and `sh::ChildGuard::id()` (#45).
- `sh::wait_until(timeout, || probe)` calls a probe until it returns
  `true`. The error names the timeout. It stops on an interrupt (#45).
