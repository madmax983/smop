//! Environment variable utilities.
//!
//! Provides typed access to environment variables with parsing,
//! defaults, and dotenv file loading.

use std::str::FromStr;

use anyhow::{Context, Result, anyhow};

/// Gets an environment variable and parses it to the specified type.
///
/// # Errors
///
/// Returns an error if:
/// - The environment variable is not set
/// - The value cannot be parsed to the specified type
///
/// # Examples
///
/// ```no_run
/// use smop::env;
///
/// // Assumes MY_PORT is set in the environment
/// let port: u16 = env::var("MY_PORT").unwrap();
/// println!("Port: {}", port);
/// ```
pub fn var<T: FromStr>(name: &str) -> Result<T>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let value =
        std::env::var(name).with_context(|| format!("Environment variable {name} not set"))?;

    value
        .parse()
        .with_context(|| format!("Failed to parse environment variable {name}"))
}

/// Gets an environment variable, returning a default if not set or unparseable.
///
/// # Examples
///
/// ```
/// use smop::env;
///
/// let timeout: u32 = env::var_or("TIMEOUT", 30);
/// assert_eq!(timeout, 30); // Uses default since TIMEOUT isn't set
/// ```
pub fn var_or<T: FromStr>(name: &str, default: T) -> T
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    var(name).unwrap_or(default)
}

/// Loads environment variables from a .env file in the current directory.
///
/// If no .env file exists, this is a no-op and returns Ok.
///
/// # Errors
///
/// Returns an error if the .env file exists but cannot be parsed.
///
/// # Examples
///
/// ```no_run
/// use smop::env;
///
/// env::dotenv().unwrap();
/// ```
pub fn dotenv() -> Result<()> {
    match dotenvy::dotenv() {
        Ok(_) => Ok(()),
        Err(dotenvy::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow!("Failed to load .env file: {e}")),
    }
}

/// Ensures all specified environment variables are set.
///
/// # Errors
///
/// Returns an error listing all missing variables.
///
/// # Examples
///
/// ```no_run
/// use smop::env;
///
/// // Ensures these environment variables are set before proceeding
/// env::require_vars(&["API_KEY", "API_URL"]).unwrap();
/// ```
pub fn require_vars(names: &[&str]) -> Result<()> {
    let missing: Vec<&str> = names
        .iter()
        .filter(|name| std::env::var(name).is_err())
        .copied()
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "Missing required environment variables: {}",
            missing.join(", ")
        ))
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // SAFETY: These tests run single-threaded and use unique env var names
    // to avoid conflicts. The env vars are cleaned up after each test.

    #[test]
    fn var_reads_and_parses_existing_env_var() {
        // SAFETY: Test-only, unique var name, cleaned up after
        unsafe { std::env::set_var("SCRIPTKIT_TEST_INT", "42") };
        let value: i32 = var("SCRIPTKIT_TEST_INT").unwrap();
        assert_eq!(value, 42);
        unsafe { std::env::remove_var("SCRIPTKIT_TEST_INT") };
    }

    #[test]
    fn var_reads_string() {
        unsafe { std::env::set_var("SCRIPTKIT_TEST_STR", "hello") };
        let value: String = var("SCRIPTKIT_TEST_STR").unwrap();
        assert_eq!(value, "hello");
        unsafe { std::env::remove_var("SCRIPTKIT_TEST_STR") };
    }

    #[test]
    fn var_fails_on_missing() {
        unsafe { std::env::remove_var("SCRIPTKIT_MISSING_VAR") };
        let result: Result<String> = var("SCRIPTKIT_MISSING_VAR");
        assert!(result.is_err());
    }

    #[test]
    fn var_fails_on_parse_error() {
        unsafe { std::env::set_var("SCRIPTKIT_BAD_INT", "not_a_number") };
        let result: Result<i32> = var("SCRIPTKIT_BAD_INT");
        assert!(result.is_err());
        unsafe { std::env::remove_var("SCRIPTKIT_BAD_INT") };
    }

    #[test]
    fn var_or_returns_value_when_set() {
        unsafe { std::env::set_var("SCRIPTKIT_TEST_OR", "100") };
        let value: i32 = var_or("SCRIPTKIT_TEST_OR", 50);
        assert_eq!(value, 100);
        unsafe { std::env::remove_var("SCRIPTKIT_TEST_OR") };
    }

    #[test]
    fn var_or_returns_default_when_missing() {
        unsafe { std::env::remove_var("SCRIPTKIT_MISSING_OR") };
        let value: i32 = var_or("SCRIPTKIT_MISSING_OR", 50);
        assert_eq!(value, 50);
    }

    #[test]
    fn var_or_returns_default_on_parse_error() {
        unsafe { std::env::set_var("SCRIPTKIT_BAD_OR", "not_a_number") };
        let value: i32 = var_or("SCRIPTKIT_BAD_OR", 99);
        assert_eq!(value, 99);
        unsafe { std::env::remove_var("SCRIPTKIT_BAD_OR") };
    }

    #[test]
    fn require_vars_succeeds_when_all_present() {
        unsafe {
            std::env::set_var("SCRIPTKIT_REQ_A", "a");
            std::env::set_var("SCRIPTKIT_REQ_B", "b");
        }
        let result = require_vars(&["SCRIPTKIT_REQ_A", "SCRIPTKIT_REQ_B"]);
        assert!(result.is_ok());
        unsafe {
            std::env::remove_var("SCRIPTKIT_REQ_A");
            std::env::remove_var("SCRIPTKIT_REQ_B");
        }
    }

    #[test]
    fn require_vars_fails_when_any_missing() {
        unsafe {
            std::env::set_var("SCRIPTKIT_REQ_C", "c");
            std::env::remove_var("SCRIPTKIT_MISSING_REQ");
        }
        let result = require_vars(&["SCRIPTKIT_REQ_C", "SCRIPTKIT_MISSING_REQ"]);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("SCRIPTKIT_MISSING_REQ"));
        unsafe { std::env::remove_var("SCRIPTKIT_REQ_C") };
    }

    #[test]
    fn dotenv_succeeds_when_no_file() {
        // Should not error when .env doesn't exist
        let result = dotenv();
        assert!(result.is_ok());
    }
}
