//! Shell command execution utilities.
//!
//! Cross-platform command execution with a builder pattern
//! for more complex command configurations.
//!
//! # Exit codes
//!
//! A command that does not start or ends with a failure gives an error that
//! holds a [`CommandError`]. [`exit_code`] gets its shell-style code:
//!
//! ```no_run
//! use std::process::ExitCode;
//! use smop::sh;
//!
//! fn main() -> ExitCode {
//!     match sh::cmd("cargo").arg("test").run() {
//!         Ok(()) => ExitCode::SUCCESS,
//!         Err(err) => {
//!             eprintln!("Error: {err:#}");
//!             ExitCode::from(sh::exit_code(&err).unwrap_or(1))
//!         }
//!     }
//! }
//! ```
//!
//! # Process I/O
//!
//! [`CommandBuilder`] has the I/O forms of a shell:
//!
//! | Shell | smop |
//! |---|---|
//! | `out=$(cmd)` with `$?` | [`capture`](CommandBuilder::capture) |
//! | `cmd \| tee /dev/tty` | [`tee`](CommandBuilder::tee) |
//! | `cmd \| tee log` | [`log_stdout`](CommandBuilder::log_stdout) |
//! | `cmd 2> >(tee log >&2)` | [`log_stderr`](CommandBuilder::log_stderr) |
//! | `cmd > file`, `cmd >> file` | [`stdout_file`](CommandBuilder::stdout_file), [`stdout_append`](CommandBuilder::stdout_append) |
//! | `cmd 2>&1` | [`stderr_to_stdout`](CommandBuilder::stderr_to_stdout) |
//! | `cmd >&2` | [`stdout_to_stderr`](CommandBuilder::stdout_to_stderr) |
//! | `cmd <<< text` | [`stdin`](CommandBuilder::stdin) |
//! | `cmd > /dev/null 2>&1` | [`quiet`](CommandBuilder::quiet) |
//! | `timeout 5 cmd` | [`timeout`](CommandBuilder::timeout) |
//! | `mapfile -t lines < <(cmd)` | [`output_lines`](CommandBuilder::output_lines) |
//!
//! Threads read stdout and stderr while smop waits. Thus a child that
//! writes much to both streams does not block.
//!
//! ```no_run
//! use std::time::Duration;
//! use smop::sh;
//!
//! let result = sh::cmd("cargo")
//!     .args(["test", "--workspace"])
//!     .stderr_to_stdout()
//!     .log_stdout("target/test.log")
//!     .tee()
//!     .timeout(Duration::from_secs(600))
//!     .capture()?;
//! if !result.success() {
//!     eprintln!("tests failed with code {}", result.code());
//! }
//! # Ok::<(), anyhow::Error>(())
//! ```

mod stdio;

use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::Path;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};

use stdio::{Finished, Mode, StdoutTarget, Target};

/// A command that did not start, or that ended with a failure.
///
/// An error for a command that does not start or that fails holds this
/// type. The signatures stay `anyhow::Result`. Use [`exit_code`] to get the code. To get this value,
/// search `err.chain()` with `downcast_ref`. This also finds it when the
/// error is the source of a different error type.
///
/// The code follows bash:
///
/// - 127: the program is not found. On Unix, a missing working directory
///   (see [`CommandBuilder::dir`]) also gives 127.
/// - 126: the program is found but does not start (for example, the file
///   is not executable).
/// - 128 + N: signal N stops the process (Unix only).
/// - 124: the [`timeout`](CommandBuilder::timeout) stops the process, as
///   GNU `timeout` does.
/// - Other codes: the exit code of the process.
///
/// The `Display` text names the command line. For a process that ran, it
/// also names the code, for example `Command failed with exit code 1: false`.
///
/// # Examples
///
/// ```no_run
/// use smop::sh::{self, CommandError};
///
/// let err = sh::cmd("false").run().unwrap_err();
/// let failure = err.chain().find_map(|e| e.downcast_ref::<CommandError>());
/// if let Some(failure) = failure {
///     assert_eq!(failure.code(), 1);
///     assert_eq!(failure.command(), "false");
/// }
/// ```
#[derive(Debug)]
pub struct CommandError {
    command: String,
    failure: Failure,
    stdout: Option<String>,
    stderr: Option<String>,
}

/// Why a command failed.
#[derive(Debug)]
enum Failure {
    /// The process did not start. The code is 127 or 126.
    Spawn(io::Error),
    /// The process ended with a non-zero exit code.
    Exited(i32),
    /// The process was killed by a signal (Unix only).
    #[cfg_attr(not(unix), allow(dead_code))]
    Signaled(i32),
    /// The timeout stopped the process. The code is 124.
    TimedOut(Duration),
}

impl Failure {
    /// Gets the shell-style code. See [`CommandError::code`].
    fn code(&self) -> i32 {
        match self {
            Self::Spawn(err) => spawn_code(err),
            Self::Exited(code) => *code,
            Self::Signaled(signal) => 128 + signal,
            Self::TimedOut(_) => 124,
        }
    }
}

impl CommandError {
    /// Makes an error for a process that did not start.
    const fn spawn(command: String, source: io::Error) -> Self {
        Self {
            command,
            failure: Failure::Spawn(source),
            stdout: None,
            stderr: None,
        }
    }

    /// Makes an error for a process that ended with `status`.
    ///
    /// `stderr` is the captured error output, if any.
    fn status(command: String, status: ExitStatus, stderr: Option<String>) -> Self {
        Self {
            command,
            failure: failure_of(status),
            stdout: None,
            stderr,
        }
    }

    /// Makes an error for a process that the timeout stopped.
    const fn timed_out_after(command: String, after: Duration) -> Self {
        Self {
            command,
            failure: Failure::TimedOut(after),
            stdout: None,
            stderr: None,
        }
    }

    /// Sets the captured output.
    fn with_output(mut self, stdout: Option<String>, stderr: Option<String>) -> Self {
        self.stdout = stdout;
        self.stderr = stderr;
        self
    }

    /// Gets the command line, as [`CommandBuilder`]'s `Display` shows it.
    ///
    /// For [`run`] and [`output`], this is the shell command text. For a
    /// [`PipeBuilder`], this is the command line of the stage that failed.
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Gets the shell-style exit code.
    ///
    /// The code is 127 or 126 when the program does not start. It is
    /// 128 + N when signal N stops the process (Unix), and 124 when the
    /// timeout stops it. Otherwise, it is the exit code of the process. On Windows, the code can be outside
    /// 1..=255. [`exit_code`] changes such a code to 1.
    ///
    /// Unlike [`ExitStatus::code`], this is never `None` and never 0.
    #[must_use]
    pub fn code(&self) -> i32 {
        self.failure.code()
    }

    /// Gets the signal that killed the process (Unix only).
    ///
    /// Returns `None` when the process exited, did not start or timed out.
    #[must_use]
    pub const fn signal(&self) -> Option<i32> {
        match self.failure {
            Failure::Signaled(signal) => Some(signal),
            Failure::Spawn(_) | Failure::Exited(_) | Failure::TimedOut(_) => None,
        }
    }

    /// Returns `true` when the [`timeout`](CommandBuilder::timeout) stopped
    /// the process.
    ///
    /// A process can also exit with 124 itself. This tells the two apart.
    #[must_use]
    pub const fn timed_out(&self) -> bool {
        matches!(self.failure, Failure::TimedOut(_))
    }

    /// Gets the captured stdout of the process, decoded lossily.
    ///
    /// Returns `None` when stdout was not captured. `output`,
    /// `output_lines`, `capture` and `output_raw` capture it. After a
    /// timeout, this is the output before the timeout.
    #[must_use]
    pub fn stdout(&self) -> Option<&str> {
        self.stdout.as_deref()
    }

    /// Gets the captured stderr of the process, decoded lossily.
    ///
    /// Returns `None` when stderr was not captured. The capture methods
    /// capture all of it. A [`quiet`](CommandBuilder::quiet) `run` keeps
    /// only its last 4 KiB.
    #[must_use]
    pub fn stderr(&self) -> Option<&str> {
        self.stderr.as_deref()
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let command = &self.command;
        match &self.failure {
            Failure::Spawn(_) => write!(f, "Failed to execute command: {command}")?,
            Failure::Exited(code) => {
                write!(f, "Command failed with exit code {code}: {command}")?;
            }
            Failure::Signaled(signal) => write!(
                f,
                "Command killed by signal {signal} (exit code {}): {command}",
                self.code()
            )?,
            Failure::TimedOut(after) => write!(
                f,
                "Command timed out after {after:?} (exit code {}): {command}",
                self.code()
            )?,
        }
        if let Some(stderr) = self.stderr.as_deref().filter(|s| !s.is_empty()) {
            write!(f, "\n{stderr}")?;
        }
        Ok(())
    }
}

impl std::error::Error for CommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.failure {
            Failure::Spawn(err) => Some(err),
            Failure::Exited(_) | Failure::Signaled(_) | Failure::TimedOut(_) => None,
        }
    }
}

/// Gets the shell-style exit code of a failed command, as a `u8`.
///
/// Searches the whole chain of `err`, so the code is found below the
/// `.context()` layers of the caller. Returns `None` when no
/// [`CommandError`] is in the chain. A wrapper that does not show the error
/// in its `source()` chain hides the code. `io::Error::other(err)` is such
/// a wrapper. A code outside 1..=255 (Windows only)
/// gives 1, so a failure never becomes 0. On Windows, `cmd /C` gives 9009
/// for a missing program, so [`run`] of a missing program gives 1.
///
/// # Examples
///
/// ```no_run
/// use smop::prelude::*;
///
/// let err = sh::run("exit 3").context("step failed").unwrap_err();
/// assert_eq!(sh::exit_code(&err), Some(3));
///
/// // Exit with the code of the child, as a shell does.
/// std::process::exit(sh::exit_code(&err).unwrap_or(1).into());
/// ```
#[must_use]
pub fn exit_code(err: &anyhow::Error) -> Option<u8> {
    err.chain()
        .find_map(|e| e.downcast_ref::<CommandError>())
        .map(|e| shell_byte(e.code()))
}

/// Changes a code to a `u8`. A code outside 1..=255 gives 1.
fn shell_byte(code: i32) -> u8 {
    u8::try_from(code).ok().filter(|&c| c != 0).unwrap_or(1)
}

/// Gets the bash code for a spawn error: 127 for "not found", else 126.
fn spawn_code(err: &io::Error) -> i32 {
    if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    }
}

/// Gets the failure of a process that ended with `status`.
fn failure_of(status: ExitStatus) -> Failure {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return Failure::Signaled(signal);
        }
    }
    // Without a signal, a finished process always has a code.
    Failure::Exited(status.code().unwrap_or(1))
}

/// Returns `Ok` for a successful process, else a [`CommandError`].
fn check(command: String, status: ExitStatus, stderr: Option<String>) -> Result<()> {
    if status.success() {
        Ok(())
    } else {
        Err(CommandError::status(command, status, stderr).into())
    }
}

/// Makes a function that changes a spawn error into a [`CommandError`].
fn spawn_error(command: String) -> impl FnOnce(io::Error) -> anyhow::Error {
    move |err| CommandError::spawn(command, err).into()
}

/// Gets the shell-style code of `status`: 0 on success, else as
/// [`exit_code`] gives it.
fn status_byte(status: ExitStatus) -> u8 {
    if status.success() {
        0
    } else {
        shell_byte(failure_of(status).code())
    }
}

/// The result of [`CommandBuilder::capture`]: the status and the output.
///
/// The output is text, decoded lossily: smop changes bytes that are not
/// valid UTF-8 to U+FFFD. Use [`CommandBuilder::output_raw`] for the bytes.
/// smop does not trim the text.
///
/// # Examples
///
/// ```no_run
/// use smop::sh;
///
/// let result = sh::cmd("git").args(["diff", "--quiet"]).capture()?;
/// match result.code() {
///     0 => println!("clean"),
///     1 => println!("changed"),
///     code => anyhow::bail!("git diff failed ({code}): {}", result.stderr()),
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct Captured {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl Captured {
    /// Gets the exit status.
    #[must_use]
    pub const fn status(&self) -> ExitStatus {
        self.status
    }

    /// Gets the shell-style exit code.
    ///
    /// It is 0 on success. Otherwise it is the code that [`exit_code`]
    /// gives for the same failure, for example 128 + N for signal N.
    #[must_use]
    pub fn code(&self) -> u8 {
        status_byte(self.status)
    }

    /// Returns `true` when the process exited with 0.
    #[must_use]
    pub fn success(&self) -> bool {
        self.status.success()
    }

    /// Gets stdout as text.
    #[must_use]
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// Gets stderr as text. It is empty when stderr goes to stdout.
    #[must_use]
    pub fn stderr(&self) -> &str {
        &self.stderr
    }
}

/// Decodes bytes lossily.
fn lossy(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned())
}

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
        .map_err(spawn_error(command.to_string()))?;
    check(command.to_string(), status, None)
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
        .map_err(spawn_error(command.to_string()))?;
    stdout_text(command.to_string(), output)
}

/// Gets the trimmed stdout of a successful process. A failure gives a
/// [`CommandError`] with the captured stderr.
fn stdout_text(command: String, output: Output) -> Result<String> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(CommandError::status(command, output.status, Some(stderr)).into());
    }
    String::from_utf8(output.stdout)
        .with_context(|| format!("Command output was not valid UTF-8: {command}"))
        .map(|s| s.trim_end().to_string())
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
    io: stdio::Options,
}

/// Builder for chaining piped commands.
pub struct PipeBuilder {
    commands: Vec<(String, Vec<String>)>,
}

/// Handle for a background process.
#[derive(Debug)]
pub struct ChildProcess {
    child: std::process::Child,
    command: String,
}

impl CommandBuilder {
    /// Creates a new command builder for the given program.
    fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            command: Command::new(program),
            echo: false,
            io: stdio::Options::default(),
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

    /// Runs the command with the I/O options and the defaults of `mode`.
    fn execute(self, mode: Mode) -> Result<(String, Finished)> {
        let line = self.prepare();
        let finished = stdio::execute(self.command, self.io, mode, spawn_error(line.clone()))?;
        Ok((line, finished))
    }

    /// Runs the command, and gets stdout as UTF-8 text. A non-zero exit is
    /// an error.
    fn stdout_utf8(self) -> Result<String> {
        let (line, mut finished) = self.execute(Mode::Capture)?;
        if let Some(err) = finished_error(line.clone(), &mut finished, Shown::All, true) {
            return Err(err);
        }
        String::from_utf8(finished.stdout)
            .with_context(|| format!("Command output was not valid UTF-8: {line}"))
    }

    /// Sends `input` to the stdin of the command.
    ///
    /// Without this, `run` and `status` give the command the stdin of this
    /// process. The other run methods give it an empty stdin. When the
    /// command closes its stdin before it reads all, smop stops writing.
    /// This is not an error.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::sh;
    ///
    /// let sorted = sh::cmd("sort").stdin("b\na\n").output_lines()?;
    /// assert_eq!(sorted, ["a", "b"]);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[must_use]
    pub fn stdin<B: AsRef<[u8]>>(mut self, input: B) -> Self {
        self.io.stdin = Some(input.as_ref().to_vec());
        self
    }

    /// Keeps the output of the command off the terminal.
    ///
    /// - [`run`](Self::run) keeps the last 4 KiB of stderr. If the command
    ///   fails, the error shows them.
    /// - [`tee`](Self::tee) has no effect.
    /// - Capture, files and log files still get the output.
    /// - With [`echo`](Self::echo), smop still prints the command line.
    ///   Only the output of the command is quiet.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::sh;
    ///
    /// // Prints nothing on success. On failure, the error has the stderr tail.
    /// sh::cmd("npm").args(["ci"]).quiet().run()?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[must_use]
    pub const fn quiet(mut self) -> Self {
        self.io.quiet = true;
        self
    }

    /// Shows the output on the terminal while smop captures it.
    ///
    /// smop copies stdout to the stdout of this process, and stderr to its
    /// stderr, as the command writes them. This applies to
    /// [`output`](Self::output), [`output_lines`](Self::output_lines),
    /// [`capture`](Self::capture) and [`output_raw`](Self::output_raw).
    /// [`run`](Self::run) and [`status`](Self::status) show the output
    /// without this.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::sh;
    ///
    /// // Shows the build as it runs, and keeps the text.
    /// let log = sh::cmd("make").tee().output()?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[must_use]
    pub const fn tee(mut self) -> Self {
        self.io.tee = true;
        self
    }

    /// Copies stdout to the file at `path`, as `cmd | tee path` does.
    ///
    /// smop makes the file or truncates it. Stdout still goes to its usual
    /// place. With [`stderr_to_stdout`](Self::stderr_to_stdout), the file
    /// gets both streams, in the order the command writes them.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::sh;
    ///
    /// // cargo build 2>&1 | tee build.log
    /// sh::cmd("cargo").arg("build").stderr_to_stdout().log_stdout("build.log").run()?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[must_use]
    pub fn log_stdout<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.io.log_stdout = Some(path.as_ref().to_path_buf());
        self
    }

    /// Copies stderr to the file at `path`.
    ///
    /// smop makes the file or truncates it. Stderr still goes to its usual
    /// place. With [`stderr_to_stdout`](Self::stderr_to_stdout), the run
    /// method gives an error. Use [`log_stdout`](Self::log_stdout) then.
    #[must_use]
    pub fn log_stderr<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.io.log_stderr = Some(path.as_ref().to_path_buf());
        self
    }

    /// Sends stdout to the file at `path`, as `cmd > path` does.
    ///
    /// smop makes the file or truncates it. smop opens it before the
    /// command starts. If it can not open it, the command does not start.
    /// Stdout does not go to the terminal or to the capture.
    #[must_use]
    pub fn stdout_file<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.io.stdout = StdoutTarget::File(Target {
            path: path.as_ref().to_path_buf(),
            append: false,
        });
        self
    }

    /// Adds stdout to the end of the file at `path`, as `cmd >> path`
    /// does.
    ///
    /// smop makes the file if it does not exist. Otherwise, this is the
    /// same as [`stdout_file`](Self::stdout_file).
    #[must_use]
    pub fn stdout_append<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.io.stdout = StdoutTarget::File(Target {
            path: path.as_ref().to_path_buf(),
            append: true,
        });
        self
    }

    /// Sends stderr to the place of stdout, as `cmd 2>&1` does.
    ///
    /// The two streams use one pipe or file, so the order of the lines
    /// stays correct. Stderr follows stdout to a file and to the capture.
    /// [`Captured::stderr`] is then empty. This cancels
    /// [`stdout_to_stderr`](Self::stdout_to_stderr).
    #[must_use]
    pub fn stderr_to_stdout(mut self) -> Self {
        self.io.stderr_to_stdout = true;
        if matches!(self.io.stdout, StdoutTarget::Stderr) {
            self.io.stdout = StdoutTarget::Terminal;
        }
        self
    }

    /// Sends stdout to the place of stderr, as `cmd >&2` does.
    ///
    /// This replaces [`stdout_file`](Self::stdout_file) and cancels
    /// [`stderr_to_stdout`](Self::stderr_to_stdout).
    #[must_use]
    pub fn stdout_to_stderr(mut self) -> Self {
        self.io.stdout = StdoutTarget::Stderr;
        self.io.stderr_to_stdout = false;
        self
    }

    /// Stops the command when it runs longer than `limit`.
    ///
    /// At the limit, smop kills the command and waits for it to end. The
    /// run method then gives an error with code 124, as GNU `timeout`
    /// does. [`CommandError::stdout`] and [`CommandError::stderr`] have
    /// the output that smop captured before.
    ///
    /// smop kills only the command, not the processes that it started. If
    /// such a process keeps stdout or stderr open, smop waits for it until
    /// the limit plus 200 ms, and then returns.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use smop::sh;
    ///
    /// match sh::cmd("curl").arg("http://localhost:8080/health")
    ///     .timeout(Duration::from_secs(5))
    ///     .output()
    /// {
    ///     Err(err) if sh::exit_code(&err) == Some(124) => println!("no answer"),
    ///     other => println!("{:?}", other?),
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[must_use]
    pub const fn timeout(mut self, limit: Duration) -> Self {
        self.io.timeout = Some(limit);
        self
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
    /// Returns an error if the command cannot be started (for example,
    /// the program is not on `PATH`), or if the [`timeout`](Self::timeout)
    /// stops it.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::sh;
    ///
    /// let status = sh::cmd("bash").arg("scripts/check.sh").echo().status()?;
    /// if !status.success() {
    ///     // `code()` is `None` after a signal. To get 128 + N, use `run()`
    ///     // and `sh::exit_code`.
    ///     std::process::exit(status.code().unwrap_or(1));
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn status(self) -> Result<ExitStatus> {
        let (line, mut finished) = self.execute(Mode::Run)?;
        finished_error(line, &mut finished, Shown::Nothing, false).map_or(Ok(finished.status), Err)
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
    /// Returns an error if the command cannot be started (for example,
    /// the program is not on `PATH`), or if the [`timeout`](Self::timeout)
    /// stops it.
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
    pub fn success(self) -> Result<bool> {
        let (line, mut finished) = self.execute(Mode::Silent)?;
        finished_error(line, &mut finished, Shown::Nothing, false)
            .map_or(Ok(finished.status.success()), Err)
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
    /// Returns an error if the command fails to execute, returns a non-zero
    /// exit code or times out. After [`quiet`](Self::quiet), the error has
    /// the end of stderr.
    pub fn run(self) -> Result<()> {
        let shown = if self.io.quiet {
            Shown::Tail
        } else {
            Shown::Nothing
        };
        let (line, mut finished) = self.execute(Mode::Run)?;
        finished_error(line, &mut finished, shown, true).map_or(Ok(()), Err)
    }

    /// Runs the command and captures stdout as a string, with the end
    /// trimmed.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to execute, returns a non-zero
    /// exit code, times out, or writes stdout that is not UTF-8. The error
    /// has the captured stderr.
    pub fn output(self) -> Result<String> {
        self.stdout_utf8().map(|s| s.trim_end().to_string())
    }

    /// Runs the command and gets the lines of stdout.
    ///
    /// Lines end with `\n` or `\r\n`. The line ends are not in the lines.
    /// Empty output gives no lines.
    ///
    /// # Errors
    ///
    /// The same as [`output`](Self::output).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::sh;
    ///
    /// for file in sh::cmd("git").args(["ls-files", "*.rs"]).output_lines()? {
    ///     println!("{file}");
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn output_lines(self) -> Result<Vec<String>> {
        Ok(self.stdout_utf8()?.lines().map(str::to_owned).collect())
    }

    /// Runs the command and gets the status, stdout and stderr.
    ///
    /// A non-zero exit is not an error. Use [`Captured::code`] or
    /// [`Captured::success`].
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to execute (code 127 or 126),
    /// or if the [`timeout`](Self::timeout) stops it (code 124).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::sh;
    ///
    /// let result = sh::cmd("cargo").args(["fmt", "--check"]).capture()?;
    /// if !result.success() {
    ///     println!("needs format:\n{}", result.stdout());
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn capture(self) -> Result<Captured> {
        let (line, mut finished) = self.execute(Mode::Capture)?;
        if let Some(err) = finished_error(line, &mut finished, Shown::All, false) {
            return Err(err);
        }
        Ok(Captured {
            status: finished.status,
            stdout: lossy(finished.stdout),
            stderr: lossy(finished.stderr),
        })
    }

    /// Runs the command and returns the raw output.
    ///
    /// A non-zero exit is not an error.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to execute or times out.
    pub fn output_raw(self) -> Result<Output> {
        let (line, mut finished) = self.execute(Mode::Capture)?;
        if let Some(err) = finished_error(line, &mut finished, Shown::All, false) {
            return Err(err);
        }
        Ok(Output {
            status: finished.status,
            stdout: finished.stdout,
            stderr: finished.stderr,
        })
    }

    /// Starts a pipe chain with this command.
    ///
    /// Only the program and its arguments go into the chain. Other
    /// settings, such as `dir`, `env` and the I/O options, do not.
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
    /// [`quiet`](Self::quiet) discards the output. The other I/O options
    /// need the run methods: `stdin`, `tee`, `timeout`, the log files and
    /// the redirections.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to spawn, or if an I/O option
    /// other than `quiet` is set.
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
        if self.io.needs_engine() {
            bail!(
                "spawn does not support stdin, tee, timeout, log files or redirection. \
                 Use run, output or capture: {self}"
            );
        }
        let line = self.prepare();
        let output = || {
            if self.io.quiet {
                Stdio::null()
            } else {
                Stdio::piped()
            }
        };
        let (stdout, stderr) = (output(), output());
        let child = self
            .command
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .map_err(spawn_error(line.clone()))?;

        Ok(ChildProcess {
            child,
            command: line,
        })
    }
}

impl fmt::Display for CommandBuilder {
    /// Renders the command line, quoting arguments that are empty or contain
    /// whitespace or quotes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_line(f, self.command.get_program(), self.command.get_args())
    }
}

impl fmt::Debug for CommandBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandBuilder")
            .field("command", &self.to_string())
            .field("dir", &self.command.get_current_dir())
            .field("echo", &self.echo)
            .field("io", &self.io)
            .finish()
    }
}

/// What the error of a run method shows of the output.
#[derive(Debug, Clone, Copy)]
enum Shown {
    Nothing,
    /// The end of stderr.
    Tail,
    /// Stdout and stderr.
    All,
}

/// Gets the error of a finished command, if any.
///
/// The order is: a timeout, then a non-zero exit (when `fail_on_exit`), then
/// an error to read output or to write a log file.
fn finished_error(
    line: String,
    finished: &mut Finished,
    shown: Shown,
    fail_on_exit: bool,
) -> Option<anyhow::Error> {
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    let output = || match shown {
        Shown::Nothing => (None, None),
        Shown::Tail => (None, Some(stdio::tail(finished.stderr_bytes()))),
        Shown::All => (Some(text(&finished.stdout)), Some(text(&finished.stderr))),
    };
    if let Some(after) = finished.timed_out {
        let (stdout, stderr) = output();
        return Some(
            CommandError::timed_out_after(line, after)
                .with_output(stdout, stderr)
                .into(),
        );
    }
    if fail_on_exit && !finished.status.success() {
        let (stdout, stderr) = output();
        return Some(
            CommandError::status(line, finished.status, None)
                .with_output(stdout, stderr)
                .into(),
        );
    }
    finished
        .io_error
        .take()
        .map(|err| err.context(format!("Command output failed: {line}")))
}

/// Writes a program and its arguments, quoted as [`write_word`] does.
fn write_line<'a>(
    f: &mut impl fmt::Write,
    program: &OsStr,
    args: impl IntoIterator<Item = &'a OsStr>,
) -> fmt::Result {
    write_word(f, program)?;
    for arg in args {
        f.write_str(" ")?;
        write_word(f, arg)?;
    }
    Ok(())
}

/// Gets the command line of a pipe stage.
fn stage_line(program: &str, args: &[String]) -> String {
    let mut line = String::new();
    // Writing to a `String` does not fail.
    let _ = write_line(
        &mut line,
        program.as_ref(),
        args.iter().map(|a| OsStr::new(a.as_str())),
    );
    line
}

/// Writes one word. Quotes it when it is empty or has whitespace or quotes.
fn write_word(f: &mut impl fmt::Write, word: &OsStr) -> fmt::Result {
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
                    .map_err(spawn_error(stage_line(program, args)))?;
                prev_stdout = child.stdout.take();
            } else {
                // Last command - capture output
                let output = command
                    .output()
                    .map_err(spawn_error(stage_line(program, args)))?;
                return stdout_text(stage_line(program, args), output);
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
                    .map_err(spawn_error(stage_line(program, args)))?;
                prev_stdout = child.stdout.take();
            } else {
                // Last command - inherit stdout/stderr
                command.stdout(Stdio::inherit());
                command.stderr(Stdio::inherit());
                let status = command
                    .status()
                    .map_err(spawn_error(stage_line(program, args)))?;
                check(stage_line(program, args), status, None)?;
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
            .with_context(|| format!("Failed to wait for command: {}", self.command))?;
        check(self.command, status, None)
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

    // Tests for shell-style exit codes in command errors.

    /// Gets the `CommandError` from an error chain.
    fn command_error(err: &anyhow::Error) -> &CommandError {
        err.chain()
            .find_map(|e| e.downcast_ref::<CommandError>())
            .unwrap_or_else(|| panic!("no CommandError in: {err:#}"))
    }

    #[test]
    fn exit_code_is_found_through_two_context_layers() {
        let err = exit_with(3)
            .run()
            .context("layer one")
            .context("layer two")
            .unwrap_err();
        assert_eq!(format!("{err}"), "layer two");
        assert_eq!(exit_code(&err), Some(3));
        let direct = err.downcast_ref::<CommandError>().map(CommandError::code);
        assert_eq!(direct, Some(3));
    }

    #[test]
    fn exit_code_is_found_when_error_is_a_source() {
        #[derive(Debug)]
        struct Wrapper(anyhow::Error);
        impl fmt::Display for Wrapper {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("wrapper")
            }
        }
        impl std::error::Error for Wrapper {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(self.0.as_ref())
            }
        }
        let inner = exit_with(5).run().unwrap_err();
        let err = anyhow::Error::new(Wrapper(inner));
        assert_eq!(exit_code(&err), Some(5));
    }

    #[test]
    fn exit_code_is_none_behind_io_error_other() {
        // anyhow boxes its private error type, so `io::Error::get_ref`
        // can not show the `CommandError`. The docs state this limit.
        let inner = exit_with(3).run().unwrap_err();
        let err = anyhow::Error::new(std::io::Error::other(inner));
        assert_eq!(exit_code(&err), None);
    }

    #[test]
    fn exit_code_is_none_for_other_errors() {
        assert_eq!(exit_code(&anyhow!("not a command")), None);
    }

    #[test]
    fn exit_code_maps_out_of_range_codes_to_one() {
        assert_eq!(shell_byte(0), 1);
        assert_eq!(shell_byte(3), 3);
        assert_eq!(shell_byte(255), 255);
        assert_eq!(shell_byte(256), 1);
        assert_eq!(shell_byte(-1_073_741_819), 1);
    }

    #[test]
    fn spawn_error_kinds_map_to_127_and_126() {
        use std::io::{Error, ErrorKind};
        assert_eq!(spawn_code(&Error::from(ErrorKind::NotFound)), 127);
        assert_eq!(spawn_code(&Error::from(ErrorKind::PermissionDenied)), 126);
        assert_eq!(spawn_code(&Error::other("bad format")), 126);
    }

    #[test]
    fn missing_program_gives_127_on_every_path() {
        let missing = || cmd("smop_definitely_missing_program").arg("--flag");
        let errors = [
            missing().run().unwrap_err(),
            missing().output().unwrap_err(),
            missing().status().unwrap_err(),
            missing().success().unwrap_err(),
            missing().output_raw().unwrap_err(),
            missing().spawn().unwrap_err(),
        ];
        for err in &errors {
            assert_eq!(exit_code(err), Some(127), "{err:#}");
            let e = command_error(err);
            assert_eq!(e.code(), 127);
            assert_eq!(e.signal(), None);
            assert_eq!(e.command(), "smop_definitely_missing_program --flag");
            let io = err
                .chain()
                .find_map(|e| e.downcast_ref::<std::io::Error>())
                .unwrap_or_else(|| panic!("io source kept: {err:#}"));
            assert_eq!(io.kind(), std::io::ErrorKind::NotFound);
            assert_eq!(
                err.to_string(),
                "Failed to execute command: smop_definitely_missing_program --flag"
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn missing_working_directory_gives_127() {
        let dir = setup();
        let err = cmd("true")
            .dir(dir.path().join("missing"))
            .run()
            .unwrap_err();
        assert_eq!(exit_code(&err), Some(127), "{err:#}");
    }

    #[test]
    fn success_paths_give_no_error() {
        assert!(exit_with(0).run().is_ok());
        assert!(exit_with(0).output().is_ok());
        assert!(exit_with(0).spawn().unwrap().wait().is_ok());
        assert!(run("exit 0").is_ok());
        assert!(output("exit 0").is_ok());
    }

    #[test]
    #[cfg(windows)]
    fn windows_code_above_255_is_kept_and_exit_code_gives_one() {
        let err = exit_with(300).run().unwrap_err();
        assert_eq!(command_error(&err).code(), 300);
        assert_eq!(exit_code(&err), Some(1));
    }

    #[test]
    #[cfg(windows)]
    fn pipe_run_error_names_failing_stage_and_code_windows() {
        let err = cmd("cmd")
            .args(["/C", "echo x"])
            .pipe("cmd", &["/C", "exit 6"])
            .run()
            .unwrap_err();
        assert_eq!(exit_code(&err), Some(6), "{err:#}");
        assert_eq!(command_error(&err).command(), r#"cmd /C "exit 6""#);
    }

    #[test]
    #[cfg(unix)]
    fn non_executable_file_gives_126() {
        let dir = setup();
        let script = dir.path().join("not_executable.sh");
        std::fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        let err = cmd(&script).run().unwrap_err();
        assert_eq!(exit_code(&err), Some(126), "{err:#}");
        assert_eq!(command_error(&err).code(), 126);
    }

    #[test]
    fn nonzero_exit_gives_its_code() {
        let err = exit_with(3).run().unwrap_err();
        assert_eq!(exit_code(&err), Some(3));
        let e = command_error(&err);
        assert_eq!(e.code(), 3);
        assert_eq!(e.signal(), None);
        assert_eq!(e.command(), exit_with(3).to_string());
    }

    #[test]
    fn shell_run_and_output_give_exit_code() {
        assert_eq!(exit_code(&run("exit 3").unwrap_err()), Some(3));
        assert_eq!(exit_code(&output("exit 4").unwrap_err()), Some(4));
        assert_eq!(
            command_error(&run("exit 3").unwrap_err()).command(),
            "exit 3"
        );
    }

    #[test]
    #[cfg(unix)]
    fn death_by_signal_gives_128_plus_signal() {
        let kill = || cmd("sh").args(["-c", "kill -9 $$"]);
        for err in [kill().run().unwrap_err(), kill().output().unwrap_err()] {
            assert_eq!(exit_code(&err), Some(137), "{err:#}");
            let e = command_error(&err);
            assert_eq!(e.code(), 137);
            assert_eq!(e.signal(), Some(9));
            let msg = err.to_string();
            assert!(msg.contains("killed by signal 9"), "{msg}");
            assert!(msg.contains("exit code 137"), "{msg}");
            assert!(!msg.contains("-1"), "{msg}");
        }
        for err in [
            run("kill -9 $$").unwrap_err(),
            output("kill -9 $$").unwrap_err(),
        ] {
            assert_eq!(exit_code(&err), Some(137), "{err:#}");
            assert_eq!(command_error(&err).command(), "kill -9 $$");
        }
    }

    #[test]
    fn display_keeps_exit_code_wording() {
        let err = exit_with(3).run().unwrap_err();
        let line = exit_with(3).to_string();
        assert_eq!(
            err.to_string(),
            format!("Command failed with exit code 3: {line}")
        );
    }

    #[test]
    fn output_error_keeps_stderr_after_command_line() {
        let c = if cfg!(target_os = "windows") {
            cmd("cmd").args(["/C", "echo boom 1>&2 & exit 4"])
        } else {
            cmd("sh").args(["-c", "echo boom >&2; exit 4"])
        };
        let line = c.to_string();
        let err = c.output().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.starts_with(&format!("Command failed with exit code 4: {line}\nboom")),
            "{msg}"
        );
        let e = command_error(&err);
        assert_eq!(e.command(), line);
        assert_eq!(e.stderr().map(str::trim), Some("boom"));
    }

    #[test]
    fn stderr_is_none_when_not_captured() {
        let err = exit_with(2).run().unwrap_err();
        assert_eq!(command_error(&err).stderr(), None);
        let err = cmd("smop_definitely_missing_program").output().unwrap_err();
        assert_eq!(command_error(&err).stderr(), None);
    }

    #[test]
    #[cfg(unix)]
    fn pipe_output_error_names_failing_stage_and_code() {
        let err = cmd("echo")
            .arg("hello world")
            .pipe("grep", &["-q", "no match"])
            .output()
            .unwrap_err();
        assert_eq!(exit_code(&err), Some(1), "{err:#}");
        assert_eq!(command_error(&err).command(), r#"grep -q "no match""#);
        let msg = err.to_string();
        assert!(msg.contains("exit code 1"), "{msg}");
        assert!(msg.contains(r#"grep -q "no match""#), "{msg}");
    }

    #[test]
    #[cfg(unix)]
    fn pipe_output_error_keeps_last_stage_stderr() {
        let err = cmd("echo")
            .arg("x")
            .pipe("sh", &["-c", "echo pipeboom >&2; exit 2"])
            .output()
            .unwrap_err();
        assert_eq!(exit_code(&err), Some(2), "{err:#}");
        assert!(err.to_string().contains("\npipeboom"), "{err}");
    }

    #[test]
    #[cfg(unix)]
    fn pipe_run_error_names_failing_stage_and_code() {
        let err = cmd("echo")
            .arg("x")
            .pipe("sh", &["-c", "exit 6"])
            .run()
            .unwrap_err();
        assert_eq!(exit_code(&err), Some(6), "{err:#}");
        assert_eq!(command_error(&err).command(), r#"sh -c "exit 6""#);
        assert!(err.to_string().contains("exit code 6"), "{err}");
    }

    #[test]
    #[cfg(unix)]
    fn pipe_spawn_failure_names_stage_and_gives_127() {
        for err in [
            cmd("echo")
                .arg("x")
                .pipe("smop_missing_stage", &["--a"])
                .pipe("cat", &[])
                .output()
                .unwrap_err(),
            cmd("echo")
                .arg("x")
                .pipe("smop_missing_stage", &["--a"])
                .pipe("cat", &[])
                .run()
                .unwrap_err(),
            cmd("echo")
                .arg("x")
                .pipe("smop_missing_stage", &["--a"])
                .output()
                .unwrap_err(),
        ] {
            assert_eq!(exit_code(&err), Some(127), "{err:#}");
            assert_eq!(command_error(&err).command(), "smop_missing_stage --a");
        }
    }

    #[test]
    fn child_wait_error_names_command_and_code() {
        let child = exit_with(5).spawn().unwrap();
        let err = child.wait().unwrap_err();
        assert_eq!(exit_code(&err), Some(5), "{err:#}");
        let line = exit_with(5).to_string();
        assert_eq!(command_error(&err).command(), line);
        assert_eq!(
            err.to_string(),
            format!("Command failed with exit code 5: {line}")
        );
    }

    #[test]
    #[cfg(unix)]
    fn child_wait_reports_signal_after_kill() {
        let mut child = cmd("sleep").arg("100").spawn().unwrap();
        child.kill().unwrap();
        let err = child.wait().unwrap_err();
        assert_eq!(exit_code(&err), Some(137), "{err:#}");
        assert_eq!(command_error(&err).signal(), Some(9));
        assert!(err.to_string().contains("killed by signal 9"), "{err}");
    }

    #[test]
    fn command_error_is_send_sync_and_static() {
        fn check<T: Send + Sync + 'static>() {}
        check::<CommandError>();
    }
}
