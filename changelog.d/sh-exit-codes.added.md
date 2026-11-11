- **Shell-style exit codes in command errors** (#34): `sh::CommandError` and
  `sh::exit_code(&err) -> Option<u8>`. Every `sh` error for a command that
  does not start or that fails holds a `CommandError`, with the command line,
  the code and the signal. The code follows bash:
  - 127: the program is not found (on Unix, a missing `.dir()` directory
    also gives 127)
  - 126: the program does not start (for example, it is not executable)
  - 128 + N: signal N stops the process (Unix)
  - Other failures: the exit code of the process

  `exit_code` searches the error chain, so it works below `.context()`
  layers. It changes a code outside 1..=255 to 1, so a failure never exits
  with 0. This is smop's first public error type. Signatures stay
  `anyhow::Result` (ADR 0001 has an amendment). Accessors: `command()`,
  `code()`, `signal()` and `stderr()` (captured by `output()` calls only).
