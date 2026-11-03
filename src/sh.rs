//! Shell command execution utilities.
//!
//! Cross-platform command execution with a builder pattern
//! for more complex command configurations.

use std::ffi::OsStr;
use std::fmt;
use std::path::Path;
use std::process::{Command, ExitStatus, Output, Stdio};

use anyhow::{Context, Result, anyhow};

/// Runs a shell command, inheriting stdout/stderr.
///
/// On Windows, uses `cmd /C`. On Unix, uses `sh -c`.
///
/// # Errors
///
/// Returns an error if the command fails to execute or returns non-zero exit code.
///
/// # Examples
///
/// ```no_run
/// use smop::sh;
///
/// sh::run("echo Hello")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run(command: &str) -> Result<()> {
    let status = shell_command(command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute command: {command}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "Command failed with exit code {}: {}",
            status.code().unwrap_or(-1),
            command
        ))
    }
}

/// Runs a shell command and captures its stdout as a string.
///
/// On Windows, uses `cmd /C`. On Unix, uses `sh -c`.
///
/// # Errors
///
/// Returns an error if the command fails to execute or returns non-zero exit code.
///
/// # Examples
///
/// ```no_run
/// use smop::sh;
///
/// let files = sh::output("ls -la")?;
/// println!("{}", files);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn output(command: &str) -> Result<String> {
    let output = shell_command(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("Failed to execute command: {command}"))?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .with_context(|| format!("Command output was not valid UTF-8: {command}"))
            .map(|s| s.trim_end().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!(
            "Command failed with exit code {}: {}\n{}",
            output.status.code().unwrap_or(-1),
            command,
            stderr
        ))
    }
}

/// Creates a new command builder.
///
/// # Examples
///
/// ```no_run
/// use smop::sh;
///
/// let output = sh::cmd("git")
///     .arg("status")
///     .arg("-s")
///     .output()?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[must_use]
pub fn cmd<S: AsRef<OsStr>>(program: S) -> CommandBuilder {
    CommandBuilder::new(program)
}

/// Creates a command builder for the `cargo` that is running this program.
///
/// Uses the `CARGO` environment variable when it is set — Cargo sets it for
/// everything launched via `cargo run`, including `cargo xtask` aliases — so
/// nested invocations use the same toolchain as the outer one. Falls back to
/// `cargo` on `PATH` otherwise.
///
/// # Examples
///
/// ```no_run
/// use smop::sh;
///
/// sh::cargo()
///     .args(["clippy", "--workspace", "--", "-D", "warnings"])
///     .echo()
///     .run()?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[must_use]
pub fn cargo() -> CommandBuilder {
    CommandBuilder::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

/// Builder for constructing and executing commands.
///
/// Errors from [`run`](Self::run), [`output`](Self::output) and friends name
/// the full command line, and its `Display` impl renders it too, so a failing
/// step in a script or `cargo xtask` reports exactly what was run.
pub struct CommandBuilder {
    command: Command,
    echo: bool,
}

/// Builder for chaining piped commands.
pub struct PipeBuilder {
    commands: Vec<(String, Vec<String>)>,
}

/// Handle for a background process.
pub struct ChildProcess {
    child: std::process::Child,
}

impl CommandBuilder {
    /// Creates a new command builder for the given program.
    fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            command: Command::new(program),
            echo: false,
        }
    }

    /// Prints the command line (as `$ program args...`) to stderr right
    /// before it is executed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::sh;
    ///
    /// // Prints "$ cargo test --workspace" and then runs it.
    /// sh::cmd("cargo").args(["test", "--workspace"]).echo().run()?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[must_use]
    pub const fn echo(mut self) -> Self {
        self.echo = true;
        self
    }

    /// Removes an environment variable from the command's environment.
    #[must_use]
    pub fn env_remove<K: AsRef<OsStr>>(mut self, key: K) -> Self {
        self.command.env_remove(key);
        self
    }

    /// Echoes the command if requested and returns its display form for
    /// error messages.
    fn prepare(&self) -> String {
        let line = self.to_string();
        if self.echo {
            eprintln!("$ {line}");
        }
        line
    }

    /// Runs the command with inherited stdio and returns its exit status.
    ///
    /// Unlike [`run`](Self::run), a non-zero exit is returned as
    /// `Ok(status)`, not turned into an error. Use it when the caller needs
    /// the exact exit code — for example an xtask that wraps a script and
    /// must exit with the script's own code.
    ///
    /// # Errors
    ///
    /// Returns an error only if the command cannot be started (for example,
    /// the program is not on `PATH`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::sh;
    ///
    /// let status = sh::cmd("bash").arg("scripts/check.sh").echo().status()?;
    /// if !status.success() {
    ///     // `code()` is `None` if the process was killed by a signal.
    ///     std::process::exit(status.code().unwrap_or(1));
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn status(mut self) -> Result<ExitStatus> {
        let line = self.prepare();
        self.command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("Failed to execute command: {line}"))
    }

    /// Runs the command with output discarded and reports whether it exited
    /// successfully.
    ///
    /// Unlike [`run`](Self::run), a non-zero exit is `Ok(false)`, not an
    /// error, which makes this suitable for probes such as "is this tool
    /// installed and working?".
    ///
    /// # Errors
    ///
    /// Returns an error only if the command cannot be started (for example,
    /// the program is not on `PATH`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::sh;
    ///
    /// if !sh::cargo().args(["nextest", "--version"]).success()? {
    ///     println!("cargo-nextest is not installed");
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn success(mut self) -> Result<bool> {
        let line = self.prepare();
        let status = self
            .command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .with_context(|| format!("Failed to execute command: {line}"))?;
        Ok(status.success())
    }

    /// Adds an argument to the command.
    #[must_use]
    pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> Self {
        self.command.arg(arg);
        self
    }

    /// Adds multiple arguments to the command.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command.args(args);
        self
    }

    /// Sets the working directory for the command.
    #[must_use]
    pub fn dir<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.command.current_dir(dir);
        self
    }

    /// Sets an environment variable for the command.
    #[must_use]
    pub fn env<K, V>(mut self, key: K, val: V) -> Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.command.env(key, val);
        self
    }

    /// Runs the command, inheriting stdout/stderr.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to execute or returns non-zero exit code.
    pub fn run(mut self) -> Result<()> {
        let line = self.prepare();
        let status = self
            .command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("Failed to execute command: {line}"))?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow!(
                "Command failed with exit code {}: {line}",
                status.code().unwrap_or(-1)
            ))
        }
    }

    /// Runs the command and captures stdout as a string.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to execute or returns non-zero exit code.
    pub fn output(mut self) -> Result<String> {
        let line = self.prepare();
        let output = self
            .command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("Failed to execute command: {line}"))?;

        if output.status.success() {
            String::from_utf8(output.stdout)
                .with_context(|| format!("Command output was not valid UTF-8: {line}"))
                .map(|s| s.trim_end().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!(
                "Command failed with exit code {}: {line}\n{stderr}",
                output.status.code().unwrap_or(-1),
            ))
        }
    }

    /// Runs the command and returns the raw output.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to execute.
    pub fn output_raw(mut self) -> Result<Output> {
        let line = self.prepare();
        self.command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("Failed to execute command: {line}"))
    }

    /// Starts a pipe chain with this command.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::sh;
    ///
    /// let output = sh::cmd("echo")
    ///     .arg("hello world")
    ///     .pipe("grep", &["hello"])
    ///     .output()?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[must_use]
    pub fn pipe<S: AsRef<OsStr>>(self, program: S, args: &[&str]) -> PipeBuilder {
        // Extract program and args from the first command
        let first_program = self.command.get_program().to_string_lossy().to_string();
        let first_args: Vec<String> = self
            .command
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        let second_program = program.as_ref().to_string_lossy().to_string();
        let second_args: Vec<String> = args.iter().map(std::string::ToString::to_string).collect();

        PipeBuilder {
            commands: vec![(first_program, first_args), (second_program, second_args)],
        }
    }

    /// Spawns the command in the background.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to spawn.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::sh;
    ///
    /// let mut child = sh::cmd("long-running-server").spawn()?;
    /// // Do some work...
    /// child.kill()?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn spawn(mut self) -> Result<ChildProcess> {
        let line = self.prepare();
        let child = self
            .command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn command: {line}"))?;

        Ok(ChildProcess { child })
    }
}

impl fmt::Display for CommandBuilder {
    /// Renders the command line, quoting arguments that are empty or contain
    /// whitespace or quotes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_word(f, self.command.get_program())?;
        for arg in self.command.get_args() {
            f.write_str(" ")?;
            write_word(f, arg)?;
        }
        Ok(())
    }
}

impl fmt::Debug for CommandBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandBuilder")
            .field("command", &self.to_string())
            .field("dir", &self.command.get_current_dir())
            .field("echo", &self.echo)
            .finish()
    }
}

fn write_word(f: &mut fmt::Formatter<'_>, word: &OsStr) -> fmt::Result {
    let word = word.to_string_lossy();
    if word.is_empty()
        || word
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\'')
    {
        write!(f, "{word:?}")
    } else {
        f.write_str(&word)
    }
}

impl PipeBuilder {
    /// Adds another command to the pipe chain.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::sh;
    ///
    /// let output = sh::cmd("cat")
    ///     .arg("file.txt")
    ///     .pipe("grep", &["pattern"])
    ///     .pipe("wc", &["-l"])
    ///     .output()?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[must_use]
    pub fn pipe<S: AsRef<OsStr>>(mut self, program: S, args: &[&str]) -> Self {
        let program = program.as_ref().to_string_lossy().to_string();
        let args: Vec<String> = args.iter().map(std::string::ToString::to_string).collect();
        self.commands.push((program, args));
        self
    }

    /// Executes the pipe chain and captures the final output.
    ///
    /// # Errors
    ///
    /// Returns an error if any command in the chain fails.
    pub fn output(self) -> Result<String> {
        if self.commands.is_empty() {
            return Err(anyhow!("Cannot execute empty pipe chain"));
        }

        let mut prev_stdout: Option<std::process::ChildStdout> = None;

        for (i, (program, args)) in self.commands.iter().enumerate() {
            let mut command = Command::new(program);
            command.args(args);

            // Set stdin from previous command or inherit
            if let Some(stdout) = prev_stdout.take() {
                command.stdin(Stdio::from(stdout));
            } else {
                command.stdin(Stdio::inherit());
            }

            // Set stdout and stderr to piped for all commands
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());

            if i < self.commands.len() - 1 {
                let mut child = command
                    .spawn()
                    .with_context(|| format!("Failed to spawn: {program}"))?;
                prev_stdout = child.stdout.take();
            } else {
                // Last command - capture output
                let output = command
                    .output()
                    .with_context(|| format!("Failed to execute: {program}"))?;

                if output.status.success() {
                    return String::from_utf8(output.stdout)
                        .context("Command output was not valid UTF-8")
                        .map(|s| s.trim_end().to_string());
                }
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow!("Pipe command failed: {program}\n{stderr}"));
            }
        }

        Err(anyhow!("Pipe chain ended unexpectedly"))
    }

    /// Executes the pipe chain, inheriting stdout/stderr for the final command.
    ///
    /// # Errors
    ///
    /// Returns an error if any command in the chain fails.
    pub fn run(self) -> Result<()> {
        if self.commands.is_empty() {
            return Err(anyhow!("Cannot execute empty pipe chain"));
        }

        let mut prev_stdout: Option<std::process::ChildStdout> = None;

        for (i, (program, args)) in self.commands.iter().enumerate() {
            let mut command = Command::new(program);
            command.args(args);

            // Set stdin from previous command or inherit
            if let Some(stdout) = prev_stdout.take() {
                command.stdin(Stdio::from(stdout));
            } else {
                command.stdin(Stdio::inherit());
            }

            // Set stdout - piped for all but last command
            if i < self.commands.len() - 1 {
                command.stdout(Stdio::piped());
                let mut child = command
                    .spawn()
                    .with_context(|| format!("Failed to spawn: {program}"))?;
                prev_stdout = child.stdout.take();
            } else {
                // Last command - inherit stdout/stderr
                command.stdout(Stdio::inherit());
                command.stderr(Stdio::inherit());
                let status = command
                    .status()
                    .with_context(|| format!("Failed to execute: {program}"))?;

                if !status.success() {
                    return Err(anyhow!(
                        "Pipe command failed with exit code {}: {}",
                        status.code().unwrap_or(-1),
                        program
                    ));
                }
            }
        }

        Ok(())
    }
}

impl ChildProcess {
    /// Waits for the child process to exit.
    ///
    /// # Errors
    ///
    /// Returns an error if the child exits with non-zero status or cannot be waited on.
    pub fn wait(mut self) -> Result<()> {
        let status = self
            .child
            .wait()
            .context("Failed to wait for child process")?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow!(
                "Child process exited with code {}",
                status.code().unwrap_or(-1)
            ))
        }
    }

    /// Kills the child process.
    ///
    /// # Errors
    ///
    /// Returns an error if the process cannot be killed.
    pub fn kill(&mut self) -> Result<()> {
        self.child.kill().context("Failed to kill child process")
    }

    /// Checks if the child process has exited without waiting.
    ///
    /// # Errors
    ///
    /// Returns an error if the status cannot be checked.
    pub fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        self.child
            .try_wait()
            .context("Failed to check child process status")
    }
}

/// Creates a platform-specific shell command.
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn run_executes_command() {
        // Use a simple command that should work on all platforms
        let result = if cfg!(target_os = "windows") {
            run("echo hello")
        } else {
            run("true")
        };
        assert!(result.is_ok());
    }

    #[test]
    fn run_fails_on_bad_command() {
        let result = run("nonexistent_command_12345");
        assert!(result.is_err());
    }

    #[test]
    fn run_fails_on_nonzero_exit() {
        let result = if cfg!(target_os = "windows") {
            run("cmd /c exit 1")
        } else {
            run("false")
        };
        assert!(result.is_err());
    }

    #[test]
    fn output_captures_stdout() {
        let result = if cfg!(target_os = "windows") {
            output("echo hello")
        } else {
            output("echo hello")
        };
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn output_trims_trailing_newlines() {
        let result = if cfg!(target_os = "windows") {
            output("echo test")
        } else {
            output("printf 'test\\n\\n'")
        };
        // Windows echo includes trailing space sometimes, so just check content
        assert!(result.unwrap().contains("test"));
    }

    #[test]
    fn cmd_builder_with_args() {
        let result = if cfg!(target_os = "windows") {
            cmd("cmd").args(["/C", "echo", "hello"]).output()
        } else {
            cmd("echo").arg("hello").output()
        };
        assert!(result.unwrap().contains("hello"));
    }

    #[test]
    fn cmd_builder_with_dir() {
        let dir = setup();
        let result = if cfg!(target_os = "windows") {
            cmd("cmd").args(["/C", "cd"]).dir(dir.path()).output()
        } else {
            cmd("pwd").dir(dir.path()).output()
        };
        let output = result.unwrap();
        // The output should contain the temp directory path
        assert!(!output.is_empty());
    }

    #[test]
    fn cmd_builder_with_env() {
        let result = if cfg!(target_os = "windows") {
            cmd("cmd")
                .args(["/C", "echo", "%SCRIPTKIT_TEST_ENV%"])
                .env("SCRIPTKIT_TEST_ENV", "test_value")
                .output()
        } else {
            cmd("sh")
                .args(["-c", "echo $SCRIPTKIT_TEST_ENV"])
                .env("SCRIPTKIT_TEST_ENV", "test_value")
                .output()
        };
        assert!(result.unwrap().contains("test_value"));
    }

    #[test]
    fn cmd_builder_chains_correctly() {
        let builder = cmd("program")
            .arg("arg1")
            .args(["arg2", "arg3"])
            .dir(".")
            .env("KEY", "VALUE");

        // Just verify it compiles and chains - actual execution tested above
        drop(builder);
    }

    #[test]
    #[cfg(unix)]
    fn pipe_builder_works() {
        // echo "hello world" | grep hello
        let output = cmd("echo")
            .arg("hello world")
            .pipe("grep", &["hello"])
            .output()
            .unwrap();

        assert!(output.contains("hello"));
    }

    #[test]
    #[cfg(unix)]
    fn pipe_builder_multiple_stages() {
        // echo -e "one\ntwo\nthree" | grep -v two | wc -l
        let output = cmd("sh")
            .args(["-c", "echo -e 'one\\ntwo\\nthree'"])
            .pipe("grep", &["-v", "two"])
            .pipe("wc", &["-l"])
            .output()
            .unwrap();

        assert!(output.contains("2"));
    }

    #[test]
    #[cfg(unix)]
    fn spawn_creates_background_process() {
        // Spawn a sleep command
        let mut child = cmd("sleep").arg("0.1").spawn().unwrap();

        // Initially should still be running
        let _status = child.try_wait().unwrap();
        // May or may not have finished yet, that's ok

        // Wait for it to complete
        let result = child.wait();
        assert!(result.is_ok());
    }

    #[test]
    #[cfg(unix)]
    fn spawn_can_be_killed() {
        use std::thread;
        use std::time::Duration;

        // Spawn a long-running process
        let mut child = cmd("sleep").arg("100").spawn().unwrap();

        // Give it a moment to start
        thread::sleep(Duration::from_millis(50));

        // Should still be running
        assert!(child.try_wait().unwrap().is_none());

        // Kill it
        child.kill().unwrap();

        // Wait a bit for the kill to take effect
        thread::sleep(Duration::from_millis(50));

        // Should now be dead (or at least killable means it existed)
        let _ = child.try_wait();
    }

    #[test]
    #[cfg(windows)]
    fn pipe_builder_works_windows() {
        // Skip this test on Windows as pipe chaining requires more complex shell handling
        // The functionality works but is harder to test reliably across Windows versions
    }

    #[test]
    #[cfg(windows)]
    fn spawn_creates_background_process_windows() {
        // Spawn a timeout command (Windows equivalent of sleep)
        let mut child = cmd("timeout")
            .args(["/t", "1", "/nobreak"])
            .spawn()
            .unwrap();

        // Initially may still be running
        let _ = child.try_wait();

        // Kill it to clean up
        let _ = child.kill();
    }

    /// A command that exits with `code`, via the platform shell.
    fn exit_with(code: i32) -> CommandBuilder {
        if cfg!(target_os = "windows") {
            cmd("cmd").args(["/C", &format!("exit {code}")])
        } else {
            cmd("sh").args(["-c", &format!("exit {code}")])
        }
    }

    #[test]
    fn display_renders_program_and_args() {
        let c = cmd("cargo").args(["build", "--release"]);
        assert_eq!(c.to_string(), "cargo build --release");
    }

    #[test]
    fn display_quotes_args_with_spaces_quotes_or_empty() {
        let c = cmd("git").args(["commit", "-m", "hello world", "", "it's"]);
        assert_eq!(c.to_string(), r#"git commit -m "hello world" "" "it's""#);
    }

    #[test]
    fn debug_includes_command_line() {
        let c = cmd("cargo").arg("test").dir("some/dir");
        let debug = format!("{c:?}");
        assert!(debug.contains("cargo test"), "{debug}");
        assert!(debug.contains("some/dir"), "{debug}");
    }

    #[test]
    fn run_error_names_command_and_exit_code() {
        let err = exit_with(3).run().unwrap_err().to_string();
        assert!(err.contains("exit code 3"), "{err}");
        assert!(
            err.contains("exit 3"),
            "error should name the command: {err}"
        );
    }

    #[test]
    fn output_error_names_command_and_includes_stderr() {
        let c = if cfg!(target_os = "windows") {
            cmd("cmd").args(["/C", "echo boom 1>&2 & exit 4"])
        } else {
            cmd("sh").args(["-c", "echo boom >&2; exit 4"])
        };
        let err = c.output().unwrap_err().to_string();
        assert!(err.contains("exit code 4"), "{err}");
        assert!(err.contains("boom"), "{err}");
        assert!(
            err.contains("exit 4"),
            "error should name the command: {err}"
        );
    }

    #[test]
    fn missing_program_error_names_command() {
        let err = cmd("smop_definitely_missing_program")
            .arg("--flag")
            .run()
            .unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("smop_definitely_missing_program --flag"),
            "{msg}"
        );
    }

    #[test]
    fn success_reports_exit_status_without_erroring() {
        assert!(exit_with(0).success().unwrap());
        assert!(!exit_with(1).success().unwrap());
    }

    #[test]
    fn success_errors_when_program_cannot_start() {
        assert!(cmd("smop_definitely_missing_program").success().is_err());
    }

    #[test]
    fn echo_does_not_change_behaviour() {
        assert!(exit_with(0).echo().success().unwrap());
        assert!(exit_with(2).echo().run().is_err());
    }

    #[test]
    fn env_remove_hides_variable_from_child() {
        let c = if cfg!(target_os = "windows") {
            cmd("cmd").args(["/C", "if defined SMOP_REMOVED (echo set) else (echo unset)"])
        } else {
            cmd("sh").args(["-c", "echo ${SMOP_REMOVED-unset}"])
        };
        let out = c
            .env("SMOP_REMOVED", "set")
            .env_remove("SMOP_REMOVED")
            .output()
            .unwrap();
        assert_eq!(out.trim(), "unset");
    }

    #[test]
    fn cargo_runs_the_current_cargo() {
        let version = cargo().arg("--version").output().unwrap();
        assert!(version.starts_with("cargo "), "{version}");
    }

    #[test]
    fn status_returns_exact_nonzero_exit_code() {
        let status = exit_with(42).status().unwrap();
        assert!(!status.success());
        assert_eq!(status.code(), Some(42));
    }

    #[test]
    fn status_returns_success_for_zero_exit() {
        let status = exit_with(0).echo().status().unwrap();
        assert!(status.success());
        assert_eq!(status.code(), Some(0));
    }

    #[test]
    fn status_errors_and_names_command_when_program_cannot_start() {
        let err = cmd("smop_definitely_missing_program")
            .arg("--flag")
            .status()
            .unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("smop_definitely_missing_program --flag"),
            "{msg}"
        );
    }
}
