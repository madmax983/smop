//! Path utilities for scripting.
//!
//! Provides convenient functions for working with paths,
//! including home directory, current directory, and path expansion.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Returns the user's home directory.
///
/// # Panics
///
/// Panics if the home directory cannot be determined.
/// This should be rare on properly configured systems.
///
/// # Examples
///
/// ```
/// use smop::path;
///
/// let home = path::home();
/// assert!(home.exists());
/// ```
#[must_use]
pub fn home() -> PathBuf {
    home_dir().unwrap_or_else(|| {
        // Fallback for edge cases - should rarely happen
        PathBuf::from(".")
    })
}

/// The user's home directory, if it can be determined.
///
/// Uses `std::env::home_dir`, which (since Rust 1.85) consults `USERPROFILE`
/// on Windows and `HOME` / the passwd database on Unix — the same sources as
/// the `dirs` crate, without the extra dependency.
fn home_dir() -> Option<PathBuf> {
    std::env::home_dir().filter(|p| !p.as_os_str().is_empty())
}

/// Returns the current working directory.
///
/// # Errors
///
/// Returns an error if the current directory cannot be determined.
///
/// # Examples
///
/// ```
/// use smop::path;
///
/// let cwd = path::cwd().unwrap();
/// assert!(cwd.is_absolute());
/// ```
pub fn cwd() -> Result<PathBuf> {
    std::env::current_dir().context("Failed to get current working directory")
}

/// Expands tilde and environment variables in a path.
///
/// - `~` is expanded to the user's home directory
/// - `$VAR` and `${VAR}` are expanded to environment variable values
///
/// # Examples
///
/// ```
/// use smop::path;
///
/// let expanded = path::expand("~/documents");
/// assert!(!expanded.to_string_lossy().contains('~'));
/// ```
#[must_use]
pub fn expand<P: AsRef<str>>(path: P) -> PathBuf {
    let input = path.as_ref();
    let expanded = shellexpand::full_with_context(
        input,
        || home_dir().map(|p| p.to_string_lossy().into_owned()),
        |var| std::env::var(var).map(Some),
    )
    .unwrap_or_else(|_| input.into());
    PathBuf::from(expanded.as_ref())
}

/// Finds an executable in the system PATH.
///
/// # Errors
///
/// Returns an error if the executable is not found in PATH.
///
/// # Examples
///
/// ```no_run
/// use smop::path;
///
/// let git_path = path::which("git")?;
/// println!("Git found at: {}", git_path.display());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn which<S: AsRef<OsStr>>(program: S) -> Result<PathBuf> {
    let program = program.as_ref();
    which::which(program).with_context(|| {
        format!(
            "Executable not found in PATH: {}",
            program.to_string_lossy()
        )
    })
}

/// Returns the root directory of the enclosing Cargo workspace.
///
/// Runs `cargo locate-project --workspace` from the current directory, using
/// the `cargo` binary from the `CARGO` environment variable when set (as it is
/// for anything launched through `cargo run`, e.g. a `cargo xtask` alias) and
/// falling back to `cargo` on `PATH`. For a crate that is not part of a
/// workspace this is the directory containing its `Cargo.toml`.
///
/// This is the runtime counterpart of the common
/// `Path::new(env!("CARGO_MANIFEST_DIR")).parent()` idiom, and keeps working
/// if the xtask crate moves.
///
/// # Errors
///
/// Returns an error if `cargo` cannot be run, or if the current directory is
/// not inside a Cargo project.
///
/// # Examples
///
/// ```no_run
/// use smop::path;
///
/// let root = path::workspace_root()?;
/// let lockfile = root.join("Cargo.lock");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn workspace_root() -> Result<PathBuf> {
    workspace_root_in(None)
}

fn workspace_root_in(dir: Option<&Path>) -> Result<PathBuf> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = std::process::Command::new(&cargo);
    command.args(["locate-project", "--workspace", "--message-format", "plain"]);
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let output = command.output().with_context(|| {
        format!(
            "Failed to run `{} locate-project --workspace`",
            cargo.to_string_lossy()
        )
    })?;

    if !output.status.success() {
        bail!(
            "Not inside a Cargo workspace (`cargo locate-project --workspace` failed): {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let manifest = String::from_utf8(output.stdout)
        .context("`cargo locate-project` printed a non-UTF-8 path")?;
    let manifest = Path::new(manifest.trim());
    manifest
        .parent()
        .map(Path::to_path_buf)
        .with_context(|| format!("Manifest path has no parent: {}", manifest.display()))
}

/// Checks if a path is executable.
///
/// On Unix, checks the executable permission bit.
/// On Windows, checks if the extension is .exe, .bat, .cmd, .com, or .ps1.
///
/// # Errors
///
/// Returns an error if the file metadata cannot be read.
///
/// # Examples
///
/// ```no_run
/// use smop::path;
///
/// if path::is_executable("/usr/bin/ls")? {
///     println!("ls is executable");
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn is_executable<P: AsRef<Path>>(path: P) -> Result<bool> {
    let path = path.as_ref();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = path
            .metadata()
            .with_context(|| format!("Failed to read metadata: {}", path.display()))?;
        Ok(metadata.permissions().mode() & 0o111 != 0)
    }

    #[cfg(windows)]
    {
        // On Windows, check if it has an executable extension
        path.extension().and_then(|e| e.to_str()).map_or_else(
            || Ok(false),
            |ext| {
                let ext_lower = ext.to_lowercase();
                Ok(matches!(
                    ext_lower.as_str(),
                    "exe" | "bat" | "cmd" | "com" | "ps1"
                ))
            },
        )
    }

    #[cfg(not(any(unix, windows)))]
    {
        // Fallback for other platforms
        Ok(false)
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;

    #[test]
    fn home_returns_existing_directory() {
        let home = home();
        assert!(home.exists(), "Home directory should exist");
        assert!(home.is_dir(), "Home should be a directory");
    }

    #[test]
    fn cwd_returns_current_directory() {
        let cwd = cwd().unwrap();
        assert!(cwd.is_absolute(), "Current directory should be absolute");
        assert!(cwd.exists(), "Current directory should exist");
    }

    #[test]
    fn expand_resolves_tilde() {
        let expanded = expand("~/test");
        let path_str = expanded.to_string_lossy();
        assert!(
            !path_str.starts_with('~'),
            "Tilde should be expanded: {path_str}"
        );
    }

    #[test]
    fn expand_resolves_env_vars() {
        // SAFETY: Test-only, unique var name, cleaned up after
        unsafe { std::env::set_var("SCRIPTKIT_TEST_VAR", "test_value") };
        let expanded = expand("$SCRIPTKIT_TEST_VAR/subdir");
        let path_str = expanded.to_string_lossy();
        assert!(
            path_str.contains("test_value"),
            "Env var should be expanded: {path_str}"
        );
        unsafe { std::env::remove_var("SCRIPTKIT_TEST_VAR") };
    }

    #[test]
    fn expand_handles_braced_env_vars() {
        // SAFETY: Test-only, unique var name, cleaned up after
        unsafe { std::env::set_var("SCRIPTKIT_BRACED", "braced_value") };
        let expanded = expand("${SCRIPTKIT_BRACED}/path");
        let path_str = expanded.to_string_lossy();
        assert!(
            path_str.contains("braced_value"),
            "Braced env var should be expanded: {path_str}"
        );
        unsafe { std::env::remove_var("SCRIPTKIT_BRACED") };
    }

    #[test]
    fn expand_preserves_regular_paths() {
        let path = "/regular/path/no/expansion";
        let expanded = expand(path);
        assert_eq!(expanded, PathBuf::from(path));
    }

    #[test]
    fn which_finds_common_command() {
        // Try to find a command that should exist on both Windows and Unix
        let result = if cfg!(target_os = "windows") {
            which("cmd")
        } else {
            which("sh")
        };
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.exists());
    }

    #[test]
    fn which_fails_for_nonexistent() {
        let result = which("nonexistent_command_xyz_12345");
        assert!(result.is_err());
    }

    #[test]
    #[cfg(unix)]
    fn is_executable_detects_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.sh");

        // Create file without execute permission
        std::fs::write(&path, "#!/bin/sh\necho test").unwrap();
        assert!(!is_executable(&path).unwrap());

        // Add execute permission
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();

        assert!(is_executable(&path).unwrap());
    }

    #[test]
    #[cfg(windows)]
    fn is_executable_detects_windows_extensions() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();

        // Test executable extensions
        let exe_path = dir.path().join("test.exe");
        std::fs::write(&exe_path, "").unwrap();
        assert_eq!(is_executable(&exe_path).unwrap(), true);

        let bat_path = dir.path().join("test.bat");
        std::fs::write(&bat_path, "").unwrap();
        assert_eq!(is_executable(&bat_path).unwrap(), true);

        let cmd_path = dir.path().join("test.cmd");
        std::fs::write(&cmd_path, "").unwrap();
        assert_eq!(is_executable(&cmd_path).unwrap(), true);

        // Test non-executable extension
        let txt_path = dir.path().join("test.txt");
        std::fs::write(&txt_path, "").unwrap();
        assert_eq!(is_executable(&txt_path).unwrap(), false);
    }

    #[test]
    fn workspace_root_finds_this_crate() {
        let root = workspace_root().unwrap();
        assert_eq!(root, Path::new(env!("CARGO_MANIFEST_DIR")));
        assert!(root.join("Cargo.toml").is_file());
    }

    #[test]
    fn workspace_root_from_subdirectory_is_the_same() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let root = workspace_root_in(Some(&src)).unwrap();
        assert_eq!(root, Path::new(env!("CARGO_MANIFEST_DIR")));
    }

    #[test]
    fn workspace_root_fails_outside_a_project() {
        let dir = tempfile::TempDir::new().unwrap();
        let err = workspace_root_in(Some(dir.path())).unwrap_err();
        assert!(
            err.to_string().contains("Not inside a Cargo workspace"),
            "unexpected error: {err}"
        );
    }
}
