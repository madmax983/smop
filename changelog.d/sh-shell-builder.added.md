- **`sh::shell(line) -> CommandBuilder`**: runs `line` through `/bin/sh -c`
  (Unix) or `cmd /C` (Windows), so `.dir()`, `.env()` and `.echo()` work on
  a shell line. `sh::run` and `sh::output` keep their signatures and wrap it.
  On Unix, extra `.arg()` values are `$0`, `$1`, and so on. The shell does
  not parse them. On Windows, they go after the line, and cmd.exe parses
  them.
