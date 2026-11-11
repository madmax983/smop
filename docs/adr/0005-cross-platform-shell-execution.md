# ADR 0005: Cross-Platform Shell Execution

## Status

Accepted

## Context

Shell command execution differs across platforms:

- **Unix**: `sh -c "command"`
- **Windows**: `cmd /C "command"`

The `sh` module must work correctly on both without user intervention.

## Decision

Implement platform-specific shell detection:

```rust
fn shell_command(command: &str) -> Command {
    if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", command]);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", command]);
        cmd
    }
}
```

Additionally, provide `CommandBuilder` for direct program execution without shell interpretation.

## Consequences

### Positive

- `sh::run("echo hello")` works on Windows and Unix
- No user code changes needed per platform
- Shell features (pipes, redirects) work naturally
- `cmd()` builder bypasses shell for performance/security

### Negative

- Shell syntax differs (Windows `%VAR%` vs Unix `$VAR`)
- Some commands have different names (`dir` vs `ls`)
- Complex scripts may not be portable

### Guidance for Users

1. **Simple commands**: Work cross-platform (`echo`, `git`, `npm`)
2. **Shell-specific syntax**: Use conditional compilation or `cfg!`
3. **Direct execution**: Use `sh::cmd("program").args([...])` for portability

### Examples

```rust
// Portable
sh::run("git status")?;
sh::cmd("rustc").arg("--version").run()?;

// Platform-specific (handle in user code)
if cfg!(windows) {
    sh::run("dir")?;
} else {
    sh::run("ls")?;
}
```

## Amendment (0.4): `sh::shell`

`sh::shell(line) -> CommandBuilder` makes the shell command. `sh::run` and
`sh::output` are `shell(line).run()` and `shell(line).output()`.

- **Windows:** the line goes through `CommandExt::raw_arg` after `/C`.
  cmd.exe does not follow the C runtime quoting that `.arg` applies.
- **Extra `.arg()`s:** on Unix they are `$0`, `$1`, …, so values get to
  the script without quoting. `cmd /C` adds them to the end of the line.
- **Display:** the builder shows the line, then the extra arguments. Thus
  `CommandError::command()` is `"exit 3"` for `run("exit 3")`, as before.

## References

- [std::process::Command](https://doc.rust-lang.org/std/process/struct.Command.html)
- [Windows cmd.exe](https://docs.microsoft.com/en-us/windows-server/administration/windows-commands/cmd)
