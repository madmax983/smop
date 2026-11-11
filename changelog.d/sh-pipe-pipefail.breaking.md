- **Pipes fail when any stage fails (pipefail)** (#43). Before, only the
  last stage counted: `false` piped into `cat` gave `Ok`. Now the error
  holds the `CommandError` of the rightmost failed stage, so
  `sh::exit_code` gives its code, as bash `pipefail` does. When two or more
  stages fail, the error text names each one. One difference from bash: on
  Unix, a stage before the last that `SIGPIPE` stops is a success, so `yes`
  piped into `head -n1` is `Ok`. Migration: a pipe that must succeed when
  an earlier stage fails needs `|| true` in that stage, for example
  `sh::shell("grep x file || true")`.
- **`pipe` waits for each stage** (#43). Before, it waited for the last
  stage only, and left the others as zombies or still running. When a
  stage does not start, smop stops the stages that started.
- **Earlier pipe stages send stderr to the terminal** (#43). Before,
  `output()` gave each stage but the last a stderr pipe that no one read,
  so the first line to stderr stopped that stage with `SIGPIPE`, and the
  pipe gave `Ok` with output cut short. The first stage still gets the
  stdin of this process.
