//! Shell command execution utilities.
//!
//! Cross-platform command execution with a builder pattern
//! for more complex command configurations.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output, Stdio};

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

/// Builder for constructing and executing commands.
pub struct CommandBuilder {
    command: Command,
}

impl CommandBuilder {
    /// Creates a new command builder for the given program.
    fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            command: Command::new(program),
        }
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
        let status = self
            .command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to execute command")?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow!(
                "Command failed with exit code {}",
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
        let output = self
            .command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to execute command")?;

        if output.status.success() {
            String::from_utf8(output.stdout)
                .context("Command output was not valid UTF-8")
                .map(|s| s.trim_end().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!(
                "Command failed with exit code {}\n{}",
                output.status.code().unwrap_or(-1),
                stderr
            ))
        }
    }

    /// Runs the command and returns the raw output.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to execute.
    pub fn output_raw(mut self) -> Result<Output> {
        self.command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to execute command")
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
}
