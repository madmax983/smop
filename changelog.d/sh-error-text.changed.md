- Error text for failed commands. Errors that said "exit code N" still do.
  - Death by signal: `Command killed by signal 9 (exit code 137): <command>`.
    Before: `exit code -1`.
  - `PipeBuilder` errors name the full command line and code of the
    rightmost stage that failed:
    `Command failed with exit code 1: grep -q "no match"`. When two or more
    stages fail, the text starts with `Pipe failed in N stages:` and names
    each stage and its code (#43). Before: `output` gave
    `Pipe command failed: grep`, and `run` gave
    `Pipe command failed with exit code 1: grep`.
  - A pipe stage that does not start gives
    `Failed to execute command: <stage>`. Before: `Failed to spawn: <program>`
    or `Failed to execute: <program>`.
  - `ChildProcess::wait` errors name the command:
    `Command failed with exit code 5: <command>`. Before:
    `Child process exited with code 5`. If the wait itself fails:
    `Failed to wait for command: <command>`. Before:
    `Failed to wait for child process`.
  - `spawn()` failures say `Failed to execute command: <command>`, as the
    other paths do. Before: `Failed to spawn command: <command>`.
  - `ChildProcess::kill` and `try_wait` errors name the command:
    `Failed to kill command: <command>` and
    `Failed to check command: <command>`. Before:
    `Failed to kill child process` and
    `Failed to check child process status` (#45).
