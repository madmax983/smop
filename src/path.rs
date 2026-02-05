//! Path utilities for scripting.
//!
//! Provides convenient functions for working with paths,
//! including home directory, current directory, and path expansion.

use std::path::PathBuf;

use anyhow::{Context, Result};

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
    dirs::home_dir().unwrap_or_else(|| {
        // Fallback for edge cases - should rarely happen
        PathBuf::from(".")
    })
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
    let expanded = shellexpand::full(path.as_ref()).unwrap_or_else(|_| path.as_ref().into());
    PathBuf::from(expanded.as_ref())
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
}
