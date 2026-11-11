- **`sh::shell(line) -> CommandBuilder`**: runs `line` through `sh -c`
  (Unix) or `cmd /C` (Windows), so `.dir()`, `.env()` and `.echo()` work on
  a shell line. `sh::run` and `sh::output` keep their signatures and wrap it.
  Extra `.arg()`s are `$0`, `$1`, … on Unix. On Windows, `cmd /C` adds them
  to the end of the line.
