- **Pipes fail when any stage fails (pipefail)** (#43). Before, only the
  last stage counted: `false` piped into `cat` gave `Ok`. Now the error
  holds the `CommandError` of the rightmost failed stage, so
  `sh::exit_code` gives its code, as bash `pipefail` does. When two or more
  stages fail, the error text names each one. One difference from bash: on
  Unix, a stage before the last that `SIGPIPE` stops is a success, so `yes`
  piped into `head -n1` is `Ok`. A stage that exits with an error code on
  a closed pipe is still a failure. Migration: a pipe that must succeed
  when a stage fails needs `|| true` in that stage, for example
  `sh::shell("grep x file || true")`. `grep` with no match exits with 1.
- **`pipe` waits for each stage** (#43). Before, it waited for the last
  stage only, and left the others as zombies or still running. When a
  stage does not start, smop stops the stages that started.
- **Earlier pipe stages send stderr to the terminal** (#43). Before,
  `output()` gave each earlier stage a stderr pipe that no one read. The
  first line to stderr stopped that stage with `SIGPIPE`. The pipe then
  gave `Ok` with output cut short. The first stage still gets the stdin of
  this process.
