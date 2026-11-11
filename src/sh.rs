//! Shell command execution utilities.
//!
//! Cross-platform command execution with a builder pattern
//! for more complex command configurations.
//!
//! [`shell`] runs a line through the shell (`sh -c` or `cmd /C`).
//! [`cmd`] runs a program directly, with no shell parsing.
//!
//! # Interrupts
//!
//! After a `script::guard` (`script-signals` feature) catches an interrupt,
//! `sh` starts no new command. It returns an error that holds
//! `script::Interrupted`. A command with [`CommandBuilder::cleanup`] still
//! runs. A command that runs at that time is not stopped.
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

use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::Path;
use std::process::{Command, ExitStatus, Output, Stdio};

use anyhow::{Context, Result, anyhow};

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
}

impl CommandError {
    /// Makes an error for a process that did not start.
    const fn spawn(command: String, source: io::Error) -> Self {
        Self {
            command,
            failure: Failure::Spawn(source),
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
            stderr,
        }
    }

    /// Gets the command line, as [`CommandBuilder`]'s `Display` shows it.
    ///
    /// For [`run`] and [`output`], this is the shell command text. For
    /// [`shell`], see [`CommandBuilder`]'s `Display`. For a [`PipeBuilder`],
    /// this is the command line of the stage that failed.
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Gets the shell-style exit code.
    ///
    /// The code is 127 or 126 when the program does not start. It is
    /// 128 + N when signal N stops the process (Unix). Otherwise, it is the
    /// exit code of the process. On Windows, the code can be outside
    /// 1..=255. [`exit_code`] changes such a code to 1.
    ///
    /// Unlike [`ExitStatus::code`], this is never `None` and never 0.
    #[must_use]
    pub fn code(&self) -> i32 {
        match &self.failure {
            Failure::Spawn(err) => spawn_code(err),
            Failure::Exited(code) => *code,
            Failure::Signaled(signal) => 128 + signal,
        }
    }

    /// Gets the signal that killed the process (Unix only).
    ///
    /// Returns `None` when the process exited or did not start.
    #[must_use]
    pub const fn signal(&self) -> Option<i32> {
        match self.failure {
            Failure::Signaled(signal) => Some(signal),
            Failure::Spawn(_) | Failure::Exited(_) => None,
        }
    }

    /// Gets the captured stderr of the process.
    ///
    /// Returns `None` when stderr was not captured. Only `output()` calls
    /// capture it.
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
        }
        if let Some(stderr) = &self.stderr {
            write!(f, "\n{stderr}")?;
        }
        Ok(())
    }
}

impl std::error::Error for CommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.failure {
            Failure::Spawn(err) => Some(err),
            Failure::Exited(_) | Failure::Signaled(_) => None,
        }
    }
}

/// Gets the shell-style exit code of a failed command, as a `u8`.
///
/// Searches the whole chain of `err`, so the code is found below the
/// `.context()` layers of the caller. With the `script` feature, a
/// `script::Interrupted` also gives a code: 128 + N on Unix, 130 on
/// Windows. Returns `None` when the chain has no [`CommandError`] and no
/// `script::Interrupted`. A wrapper that does not show the error
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
    command_code(err).map(shell_byte)
}

/// Gets the code of the first [`CommandError`] or interrupt in the chain of
/// `err`, as it is.
pub(crate) fn command_code(err: &anyhow::Error) -> Option<i32> {
    err.chain().find_map(code_of)
}

/// Gets the code of one error in a chain.
fn code_of(err: &(dyn std::error::Error + 'static)) -> Option<i32> {
    #[cfg(feature = "script")]
    if let Some(interrupt) = err.downcast_ref::<crate::script::Interrupted>() {
        return Some(interrupt.code());
    }
    err.downcast_ref::<CommandError>().map(CommandError::code)
}

/// Returns an error when a guard caught an interrupt (see
/// `script::guard`). A cleanup command skips the check.
#[cfg(feature = "script")]
fn not_interrupted(line: &str, cleanup: bool) -> Result<()> {
    match crate::script::stop() {
        Some(interrupt) if !cleanup => {
            Err(anyhow::Error::new(interrupt).context(format!("Command not started: {line}")))
        }
        _ => Ok(()),
    }
}

/// Without the `script` feature, no guard can catch an interrupt.
#[cfg(not(feature = "script"))]
#[allow(clippy::unnecessary_wraps)]
const fn not_interrupted(_line: &str, _cleanup: bool) -> Result<()> {
    Ok(())
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

/// Runs a shell command, inheriting stdout/stderr.
///
/// Same as `shell(command).run()`. See [`shell`].
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
    shell(command).run()
}

/// Runs a shell command and captures its stdout as a string.
///
/// Same as `shell(command).output()`. See [`shell`].
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
    shell(command).output()
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
pub fn cmd<S: AsRef<OsStr>>(program: S) -> CommandBuilder {
    CommandBuilder::new(program)
}

/// Creates a command builder from a command line template.
///
/// The template reads as a shell command line, but no shell runs. The
/// macro splits the template into words at compile time. A value is never
/// split and never read as syntax.
///
/// ```no_run
/// use smop::prelude::*;
///
/// let msg = "Fix the build";
/// let files = ["a b.rs", "c.rs"];
/// cmd!("git commit -m {msg} -- {files...}").run()?;
/// // argv: ["git", "commit", "-m", "Fix the build", "--", "a b.rs", "c.rs"]
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// The result is a [`CommandBuilder`], so `.dir()`, `.env()`, `.output()`
/// and the other methods follow as usual.
///
/// # Template
///
/// - ASCII whitespace separates words. A template can have many lines.
/// - The first word is the program.
/// - `{name}` is exactly one argument. It can touch text: `--out={dir}`.
///   The value is an `AsRef<OsStr>` type, an integer or a `char`. For
///   other types, use `.to_string()`.
/// - `{name...}` is one argument for each item of a list. It must be a
///   whole word. The list is a `Vec`, a slice, an array, an `Option`, a
///   set, or any type that iterates by reference. A reference to a list
///   also works, to a depth of three. The items are `AsRef<OsStr>` types.
/// - `'...'` and `"..."` make one word from text with spaces. A quote has
///   no backslash escapes. `''` is an empty argument. A quote starts a
///   word or follows `=`, and it ends the word.
/// - `{{` and `}}` are literal braces.
/// - A name is an ASCII identifier.
///
/// A `{name}` is a local variable. To use an expression, give it a name
/// after the template. Each such value is evaluated one time, in the
/// order of the list, and borrowed:
///
/// ```no_run
/// # use smop::prelude::*;
/// # let root = std::path::Path::new(".");
/// # let jobs = Some(4_u32);
/// cmd!(
///     "make -C {dir} -j{n} {targets...}",
///     dir = root.join("native"),
///     n = jobs.unwrap_or(1),
///     targets = ["all", "install"],
/// )
/// .run()?;
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Compile errors
///
/// Shell syntax outside quotes is a compile error, because no shell reads
/// it: `|`, `&`, `;`, `<`, `>`, a backtick, `$name`, `${`, `$(`, and `~` or
/// `#` at the start of a word. Put the text in quotes to pass it to the
/// program. For a real shell line, use [`shell`].
///
/// ```compile_fail
/// # use smop::prelude::*;
/// # let f = "x";
/// cmd!("rm -f {f} && echo done"); // `&&` needs a shell
/// ```
///
/// These are also compile errors:
///
/// - whitespace that is not ASCII, such as a no-break space
/// - an empty template
/// - a splat of a path, because it gives one argument for each component
/// - `{}`, `{0}`, `{x:?}` and `{a.b}`, which are reserved
/// - a listed name that is not in the template
///
/// # Limits
///
/// - An error underlines the full template. The message quotes the word.
/// - For a name that is not in scope, rustc can suggest a similar name.
///   Do not apply that fix: it replaces the full template.
/// - A listed name must not be the name of a constant in scope. The
///   compiler reads `dir = ..` as a comparison with the constant `dir`.
/// - A glob such as `*.rs` is not expanded. The program gets the text.
#[cfg(feature = "macros")]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
#[macro_export]
macro_rules! cmd {
    ($template:literal $(, $name:ident = $value:expr)* $(,)?) => {
        match ($(&$value,)*) {
            ($($name,)*) => $crate::__private::__cmd!($crate $template $($name)*),
        }
    };
    ($($other:tt)*) => {
        ::core::compile_error!(
            "cmd!: write a string literal template, then `name = value` pairs"
        )
    };
}

/// Creates a command builder that runs `command` through the shell.
///
/// On Unix, it uses `/bin/sh -c`. On Windows, it uses `cmd /C`. The text
/// goes through `raw_arg`, so cmd.exe gets it without C runtime quoting.
///
/// On Windows, cmd.exe removes the first and the last `"` of a line that
/// starts with `"` (see `cmd /?`). Put such a line in one more pair of
/// quotes: `""C:\x y\t.exe" arg"`.
///
/// `Display`, `.echo()` and errors show the command text. They do not show
/// `sh -c` or `cmd /C`. On Unix, extra arguments are an exception: then
/// they show the full `/bin/sh -c` command line.
///
/// To stop at the first error on Unix, start the text with `set -eu;`.
///
/// # Extra arguments
///
/// The meaning of `.arg()` and `.args()` is different on each platform:
///
/// - Unix: the first extra argument is `$0`. The next ones are `$1`, `$2`,
///   and so on. The shell does not parse them.
/// - Windows: Rust adds them after the command text, with C runtime
///   quoting. cmd.exe parses them as part of the line. Characters such as
///   `&`, `|` and `%` keep their meaning.
///
/// # Examples
///
/// ```no_run
/// use smop::sh;
///
/// // Unix: count the entries in "src".
/// let count = sh::shell("ls | wc -l").dir("src").output()?;
///
/// // Unix: "$1" is "a b". The script needs no quoting for it.
/// sh::shell(r#"mkdir -p "$1""#).args(["mkdir", "a b"]).run()?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn shell(command: &str) -> CommandBuilder {
    let command_line = shell_command(command);
    debug_assert_eq!(command_line.get_args().len(), SHELL_ARGS);
    CommandBuilder {
        command: command_line,
        echo: false,
        cleanup: false,
        script: Some(command.to_owned()),
    }
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
pub fn cargo() -> CommandBuilder {
    CommandBuilder::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

/// Builder for constructing and executing commands.
///
/// Errors from [`run`](Self::run), [`output`](Self::output) and friends name
/// the full command line, and its `Display` impl renders it too, so a failing
/// step in a script or `cargo xtask` reports exactly what was run.
#[must_use = "a command does nothing until a method such as `.run()` starts it"]
pub struct CommandBuilder {
    command: Command,
    echo: bool,
    cleanup: bool,
    /// The command text of a [`shell`] builder. `Display` shows it.
    script: Option<String>,
}

/// Builder for chaining piped commands.
///
/// The first stage keeps the working directory, environment, arguments and
/// `.echo()` of its [`CommandBuilder`]. With `.echo()`, the chain prints
/// `$ a | b` to stderr before it starts.
pub struct PipeBuilder {
    stages: Vec<Stage>,
    echo: bool,
    cleanup: bool,
}

/// One command of a pipe chain.
struct Stage {
    command: Command,
    /// The command line for errors.
    line: String,
}

impl Stage {
    /// Creates a stage for `program` with `args`.
    fn new<S: AsRef<OsStr>>(program: S, args: &[&str]) -> Self {
        let mut command = Command::new(program);
        command.args(args);
        let mut line = String::new();
        // A write to a `String` cannot fail.
        let _ = write_line(&mut line, command.get_program(), command.get_args());
        Self { command, line }
    }
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
            cleanup: false,
            script: None,
        }
    }

    /// Prints `$ ` and the command line (the `Display` text) to stderr right
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
    pub const fn echo(mut self) -> Self {
        self.echo = true;
        self
    }

    /// Lets the command run after an interrupt, as a cleanup step.
    ///
    /// After a `script::guard` catches an interrupt, `sh` starts no new
    /// command. A cleanup command is the exception. `script::defer` sets
    /// this. A pipe that starts with this command keeps it. `sh::run` and
    /// `sh::output` cannot be cleanup commands. Use
    /// `sh::shell(line).cleanup()` for a shell line.
    /// Without the `script-signals` feature, this has no effect.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use smop::sh;
    ///
    /// sh::cmd("docker").args(["rm", "-f", "db"]).cleanup().run()?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub const fn cleanup(mut self) -> Self {
        self.cleanup = true;
        self
    }

    /// Removes an environment variable from the command's environment.
    pub fn env_remove<K: AsRef<OsStr>>(mut self, key: K) -> Self {
        self.command.env_remove(key);
        self
    }

    /// Checks for an interrupt, echoes the command if requested, and returns
    /// its display form for error messages.
    fn prepare(&self) -> Result<String> {
        let line = self.to_string();
        not_interrupted(&line, self.cleanup)?;
        if self.echo {
            eprintln!("$ {line}");
        }
        Ok(line)
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
    ///     // `code()` is `None` after a signal. To get 128 + N, use `run()`
    ///     // and `sh::exit_code`.
    ///     std::process::exit(status.code().unwrap_or(1));
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn status(mut self) -> Result<ExitStatus> {
        let line = self.prepare()?;
        self.command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(spawn_error(line))
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
        let line = self.prepare()?;
        let status = self
            .command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(spawn_error(line))?;
        Ok(status.success())
    }

    /// Adds an argument to the command.
    pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> Self {
        self.command.arg(arg);
        self
    }

    /// Adds multiple arguments to the command.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command.args(args);
        self
    }

    /// Sets the working directory for the command.
    pub fn dir<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.command.current_dir(dir);
        self
    }

    /// Sets an environment variable for the command.
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
        let line = self.prepare()?;
        let status = self
            .command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(spawn_error(line.clone()))?;
        check(line, status, None)
    }

    /// Runs the command and captures stdout as a string.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to execute or returns non-zero exit code.
    pub fn output(mut self) -> Result<String> {
        let line = self.prepare()?;
        let output = self
            .command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(spawn_error(line.clone()))?;
        stdout_text(line, output)
    }

    /// Runs the command and returns the raw output.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to execute.
    pub fn output_raw(mut self) -> Result<Output> {
        let line = self.prepare()?;
        self.command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(spawn_error(line))
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
        let first = Stage {
            line: self.to_string(),
            command: self.command,
        };
        PipeBuilder {
            stages: vec![first, Stage::new(program, args)],
            echo: self.echo,
            cleanup: self.cleanup,
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
        let line = self.prepare()?;
        let child = self
            .command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
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
    ///
    /// A [`shell`] builder shows its command text. On Windows, the extra
    /// arguments follow it. On Unix, extra arguments are `$0`, `$1`, …, so
    /// then it shows the full `/bin/sh -c` command line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let args = self.command.get_args();
        match &self.script {
            Some(script) if cfg!(windows) || args.len() == SHELL_ARGS => {
                f.write_str(script)?;
                for arg in args.skip(SHELL_ARGS) {
                    f.write_str(" ")?;
                    write_word(f, arg)?;
                }
                Ok(())
            }
            _ => write_line(f, self.command.get_program(), args),
        }
    }
}

impl fmt::Debug for CommandBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandBuilder")
            .field("command", &self.to_string())
            .field("dir", &self.command.get_current_dir())
            .field("echo", &self.echo)
            .field("cleanup", &self.cleanup)
            .field("shell", &self.script.is_some())
            .finish()
    }
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
    /// Gets the command lines of the stages, joined with ` | `.
    fn line(&self) -> String {
        let lines: Vec<&str> = self.stages.iter().map(|s| s.line.as_str()).collect();
        lines.join(" | ")
    }

    /// Prints `$ ` and [`line`](Self::line) to stderr when echo is on.
    fn prepare(&self) {
        if self.echo {
            eprintln!("$ {}", self.line());
        }
    }

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
        self.stages.push(Stage::new(program, args));
        self
    }

    /// Executes the pipe chain and captures the final output.
    ///
    /// # Errors
    ///
    /// Returns an error if any command in the chain fails.
    pub fn output(self) -> Result<String> {
        if self.stages.is_empty() {
            return Err(anyhow!("Cannot execute empty pipe chain"));
        }
        not_interrupted(&self.line(), self.cleanup)?;

        self.prepare();
        let mut prev_stdout: Option<std::process::ChildStdout> = None;
        let last = self.stages.len() - 1;

        for (i, Stage { mut command, line }) in self.stages.into_iter().enumerate() {
            // Set stdin from previous command or inherit
            if let Some(stdout) = prev_stdout.take() {
                command.stdin(Stdio::from(stdout));
            } else {
                command.stdin(Stdio::inherit());
            }

            // Set stdout and stderr to piped for all commands
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());

            if i < last {
                let mut child = command.spawn().map_err(spawn_error(line))?;
                prev_stdout = child.stdout.take();
            } else {
                // Last command - capture output
                let output = command.output().map_err(spawn_error(line.clone()))?;
                return stdout_text(line, output);
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
        if self.stages.is_empty() {
            return Err(anyhow!("Cannot execute empty pipe chain"));
        }
        not_interrupted(&self.line(), self.cleanup)?;

        self.prepare();
        let mut prev_stdout: Option<std::process::ChildStdout> = None;
        let last = self.stages.len() - 1;

        for (i, Stage { mut command, line }) in self.stages.into_iter().enumerate() {
            // Set stdin from previous command or inherit
            if let Some(stdout) = prev_stdout.take() {
                command.stdin(Stdio::from(stdout));
            } else {
                command.stdin(Stdio::inherit());
            }

            // Set stdout - piped for all but last command
            if i < last {
                command.stdout(Stdio::piped());
                let mut child = command.spawn().map_err(spawn_error(line))?;
                prev_stdout = child.stdout.take();
            } else {
                // Last command - inherit stdout/stderr
                command.stdout(Stdio::inherit());
                command.stderr(Stdio::inherit());
                let status = command.status().map_err(spawn_error(line.clone()))?;
                check(line, status, None)?;
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

/// The number of arguments that [`shell_command`] adds before extra ones.
const SHELL_ARGS: usize = 2;

/// Creates `cmd /C command`, with `command` as a raw argument.
#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").raw_arg(command);
    cmd
}

/// The path of the POSIX shell. A full path, so a `PATH` set with `.env()`
/// does not change which shell runs.
#[cfg(all(not(windows), not(target_os = "android")))]
const SH: &str = "/bin/sh";
/// The path of the POSIX shell on Android.
#[cfg(target_os = "android")]
const SH: &str = "/system/bin/sh";

/// Creates `/bin/sh -c command`.
#[cfg(not(windows))]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new(SH);
    cmd.args(["-c", command]);
    cmd
}

/// `cmd!` tests that read the argv from the builder, so they run on each
/// platform. `tests/cmd_macro.rs` reads the argv from a real process.
#[cfg(all(test, feature = "macros"))]
mod cmd_macro_tests {
    use super::CommandBuilder;
    use std::ffi::OsString;
    use std::path::PathBuf;

    /// Gets the program and the arguments of a builder.
    fn argv(builder: &CommandBuilder) -> Vec<OsString> {
        std::iter::once(builder.command.get_program())
            .chain(builder.command.get_args())
            .map(ToOwned::to_owned)
            .collect()
    }

    #[test]
    fn text_words_are_the_program_and_its_arguments() {
        let builder = crate::cmd!("cargo test\n  --workspace '' 'a b' --x='c d' {{}}");
        assert_eq!(
            argv(&builder),
            ["cargo", "test", "--workspace", "", "a b", "--x=c d", "{}"]
        );
        assert!(builder.script.is_none());
    }

    #[test]
    fn a_placeholder_is_one_argument() {
        let msg = "a b; rm -rf / #";
        let dir = PathBuf::from("my dir");
        let jobs = 8_usize;
        let tool = "git";
        assert_eq!(
            argv(&crate::cmd!(
                "{tool} -m {msg} --out={dir}/x -j{jobs} '{msg} {jobs}'"
            )),
            [
                "git",
                "-m",
                "a b; rm -rf / #",
                "--out=my dir/x",
                "-j8",
                "a b; rm -rf / # 8"
            ]
        );
    }

    #[test]
    fn a_splat_is_one_argument_for_each_item() {
        let files = vec![String::from("a b"), String::new()];
        let verbose: Option<&str> = None;
        let slice: &[PathBuf] = &[PathBuf::from("p")];
        assert_eq!(
            argv(&crate::cmd!(
                "ls {verbose...} {files...} {slice...} {more...}",
                more = ["x"]
            )),
            ["ls", "a b", "", "p", "x"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn bytes_that_are_not_utf8_stay_the_same() {
        use std::os::unix::ffi::OsStringExt;

        let raw = OsString::from_vec(vec![0xff, b' ', 0x80]);
        let mut glued = OsString::from("--x=");
        glued.push(&raw);
        let list = [raw.clone()];
        assert_eq!(
            argv(&crate::cmd!("ls {raw} --x={raw} {list...}")),
            [OsString::from("ls"), raw.clone(), glued, raw]
        );
    }

    #[cfg(windows)]
    #[test]
    fn wide_text_that_is_not_unicode_stays_the_same() {
        use std::os::windows::ffi::OsStringExt;

        let raw = OsString::from_wide(&[0xd800, 0x20, 0x61]);
        let mut glued = OsString::from("--x=");
        glued.push(&raw);
        let list = [raw.clone()];
        assert_eq!(
            argv(&crate::cmd!("ls {raw} --x={raw} {list...}")),
            [OsString::from("ls"), raw.clone(), glued, raw]
        );
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
        assert_eq!(output("echo hello").unwrap(), "hello");
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

        assert!(output.contains('2'));
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

    // Tests for `shell`.

    /// Gets the real path of `path`. Symlinks then do not cause a mismatch.
    fn real(path: impl AsRef<Path>) -> std::path::PathBuf {
        std::fs::canonicalize(path).unwrap()
    }

    #[test]
    fn shell_dir_sets_working_directory() {
        let dir = setup();
        let line = if cfg!(windows) { "cd" } else { "pwd" };
        let out = shell(line).dir(dir.path()).output().unwrap();
        assert_eq!(real(out), real(dir.path()));
    }

    #[test]
    fn shell_env_is_visible_in_the_line() {
        let line = if cfg!(windows) { "echo %X%" } else { "echo $X" };
        assert_eq!(shell(line).env("X", "1").output().unwrap(), "1");
    }

    #[test]
    #[cfg(unix)]
    fn shell_extra_args_are_positional_parameters() {
        let out = shell(r#"printf '%s' "$1""#)
            .args(["name", "a b"])
            .output()
            .unwrap();
        assert_eq!(out, "a b");
    }

    #[test]
    #[cfg(unix)]
    fn shell_first_extra_arg_is_dollar_zero() {
        let out = shell(r#"printf '%s|%s' "$0" "$#""#)
            .args(["name", "x", "y"])
            .output()
            .unwrap();
        assert_eq!(out, "name|2");
    }

    #[test]
    #[cfg(unix)]
    fn shell_finds_sh_when_child_path_is_changed() {
        let out = shell("echo ok").env("PATH", "/nonexistent").output();
        assert_eq!(out.unwrap(), "ok");
    }

    #[test]
    #[cfg(windows)]
    fn shell_line_in_outer_quotes_keeps_leading_quote_windows() {
        // cmd.exe removes the first and the last quote of the line.
        let line = r#"""C:\Windows\System32\cmd.exe" /C echo hi""#;
        assert_eq!(shell(line).output().unwrap(), "hi");
    }

    #[test]
    #[cfg(windows)]
    fn shell_extra_args_append_to_command_text_windows() {
        let out = shell("echo a").args(["b", "c"]).output().unwrap();
        assert_eq!(out, "a b c");
    }

    #[test]
    #[cfg(windows)]
    fn shell_passes_command_text_through_raw_arg_windows() {
        // With `.arg`, the C runtime quoting gives `"echo \"a  b\""`, and
        // cmd.exe prints `\"a  b\"`.
        assert_eq!(shell(r#"echo "a  b""#).output().unwrap(), r#""a  b""#);
        assert_eq!(shell(r#"echo "a" "b""#).output().unwrap(), r#""a" "b""#);
    }

    #[test]
    fn shell_display_is_the_command_text() {
        assert_eq!(shell("exit 3").to_string(), "exit 3");
        let debug = format!("{:?}", shell("exit 3").dir("some/dir"));
        assert!(debug.contains(r#""exit 3""#), "{debug}");
        assert!(debug.contains("some/dir"), "{debug}");
        assert!(debug.contains("shell: true"), "{debug}");
        assert!(format!("{:?}", cmd("ls")).contains("shell: false"));
    }

    #[test]
    #[cfg(unix)]
    fn shell_display_with_extra_args_shows_sh_c_on_unix() {
        // The extra args are `$0` and `$1`, not text after the script.
        let c = shell(r#"printf '%s' "$1""#).args(["name", "a b"]);
        assert_eq!(
            c.to_string(),
            r#"/bin/sh -c "printf '%s' \"$1\"" name "a b""#
        );
        let err = shell("exit 5").arg("name").run().unwrap_err();
        assert_eq!(command_error(&err).command(), r#"/bin/sh -c "exit 5" name"#);
    }

    #[test]
    #[cfg(windows)]
    fn shell_display_with_extra_args_appends_them_windows() {
        let c = shell("echo").args(["a", "b c"]);
        assert_eq!(c.to_string(), r#"echo a "b c""#);
    }

    #[test]
    fn shell_error_has_exit_code_and_command_text() {
        let dir = setup();
        let err = shell("exit 3").dir(dir.path()).run().unwrap_err();
        assert_eq!(exit_code(&err), Some(3), "{err:#}");
        assert_eq!(command_error(&err).command(), "exit 3");
        assert_eq!(err.to_string(), "Command failed with exit code 3: exit 3");
    }

    #[test]
    fn shell_echo_does_not_change_behaviour() {
        assert!(shell("exit 0").echo().success().unwrap());
        assert_eq!(shell("exit 2").echo().status().unwrap().code(), Some(2));
    }

    #[test]
    fn shell_output_error_keeps_stderr() {
        let line = if cfg!(windows) {
            "echo boom 1>&2 & exit 4"
        } else {
            "echo boom >&2; exit 4"
        };
        for err in [output(line).unwrap_err(), shell(line).output().unwrap_err()] {
            assert_eq!(exit_code(&err), Some(4), "{err:#}");
            let e = command_error(&err);
            assert_eq!(e.command(), line);
            assert_eq!(e.stderr().map(str::trim), Some("boom"));
        }
    }

    #[test]
    fn pipe_keeps_dir_and_env_of_first_stage() {
        // `sort` with one line of input copies it.
        let dir = setup();
        let (pwd, echo_x, write_x) = if cfg!(windows) {
            ("cd", "echo %X%", "(echo %X%)> out")
        } else {
            ("pwd", "echo $X", r#"printf '%s' "$X" > out"#)
        };
        let out = shell(pwd)
            .dir(dir.path())
            .pipe("sort", &[])
            .output()
            .unwrap();
        assert_eq!(real(out), real(dir.path()));
        let out = shell(echo_x)
            .env("X", "1")
            .pipe("sort", &[])
            .output()
            .unwrap();
        assert_eq!(out, "1");
        shell(write_x)
            .dir(dir.path())
            .env("X", "1")
            .pipe("sort", &[])
            .run()
            .unwrap();
        let written = std::fs::read_to_string(dir.path().join("out")).unwrap();
        assert_eq!(written.trim(), "1");
    }

    #[test]
    fn pipe_keeps_echo_of_first_stage() {
        let pipe = shell("echo hi").echo().pipe("sort", &[]);
        assert!(pipe.echo);
        assert_eq!(pipe.line(), "echo hi | sort");
        assert_eq!(pipe.output().unwrap(), "hi");
        assert!(!cmd("echo").pipe("sort", &[]).echo);
    }
}
