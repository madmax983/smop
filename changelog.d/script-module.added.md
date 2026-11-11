- `script` feature (in `full`, no dependencies): `script::main(|| ..)` prints
  the error chain (`error:`, then `caused by:` lines) and exits with the
  code of a failed `sh` command, or with 1. `script::Main::error_code`
  sets the code for an error that has no code. `script::defer(cmd)` runs a
  cleanup command on drop.
  `script::retry(attempts, backoff, f)` waits `backoff`, `2 × backoff`, …
  between attempts. `script::timed(label, f)` prints `<label>: <secs>s`.
- `script-signals` feature (in `full`, adds `signal-hook`): `script::guard()`
  catches Ctrl-C, SIGTERM and SIGHUP while it lives. After an interrupt,
  `sh` starts no new command and gives `script::Interrupted`. When the
  script function returns, its `defer` commands run, then `script::main`
  stops by the same signal (Unix) or exits with 130 (Windows). A second
  signal gets its default action. The handlers stay installed, so a signal
  ignored at start (`nohup`) is caught, and later gets its default action.
- `sh::CommandBuilder::cleanup()`: the command also runs after an
  interrupt. `sh::exit_code` gives 128 + N (130 on Windows) for
  `script::Interrupted`.
