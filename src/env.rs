//! Environment variable utilities.
//!
//! Provides typed access to environment variables with parsing,
//! defaults, and dotenv file loading.
//!
//! # Unset and empty values
//!
//! [`var_opt`] and [`var_or`] read an empty value as unset, as the shell
//! does in `${NAME:-default}`. [`var`] reads an empty value as is. A value
//! that is set but does not parse is always an error. A value that is not
//! valid UTF-8 is always an error.

use std::env::VarError;
use std::ffi::OsString;
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};

/// Gets an environment variable and parses it to the specified type.
///
/// An empty value is passed to the parser as is.
///
/// # Errors
///
/// Returns an error if:
/// - The environment variable is not set
/// - The value is not valid UTF-8
/// - The value cannot be parsed to the specified type. The error names the
///   variable and the value.
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
    match read(name)? {
        Some(value) => parse(name, &value),
        None => bail!("Environment variable {name} not set"),
    }
}

/// Gets an environment variable, or `None` if it is unset or empty.
///
/// # Errors
///
/// Returns an error if the value is set but is not valid UTF-8, or cannot
/// be parsed to the specified type. The error names the variable and the
/// value.
///
/// # Examples
///
/// ```
/// use smop::env;
///
/// let level: Option<u8> = env::var_opt("SMOP_DOC_LEVEL")?;
/// assert_eq!(level, None); // SMOP_DOC_LEVEL is not set
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn var_opt<T: FromStr>(name: &str) -> Result<Option<T>>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    read(name)?
        .filter(|value| !value.is_empty())
        .map(|value| parse(name, &value))
        .transpose()
}

/// Gets an environment variable, or `default` if it is unset or empty.
///
/// The default does not apply to a value that does not parse.
///
/// # Errors
///
/// Returns an error if the value is set but is not valid UTF-8, or cannot
/// be parsed to the specified type. The error names the variable and the
/// value.
///
/// # Examples
///
/// ```
/// use smop::env;
///
/// let timeout: u32 = env::var_or("TIMEOUT", 30)?;
/// assert_eq!(timeout, 30); // Uses default since TIMEOUT isn't set
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn var_or<T: FromStr>(name: &str, default: T) -> Result<T>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    Ok(var_opt(name)?.unwrap_or(default))
}

/// Returns `true` if the process runs in a CI system.
///
/// It reads these variables:
/// - `CI`: `false` or `0` (any case) gives `false`. Other non-empty values
///   give `true`.
/// - If `CI` is unset or empty, a non-empty value in one of these gives
///   `true`: `TF_BUILD` (Azure Pipelines), `JENKINS_URL` (Jenkins),
///   `TEAMCITY_VERSION` (`TeamCity`), `CODEBUILD_BUILD_ID` (AWS `CodeBuild`).
///
/// # Examples
///
/// ```
/// use smop::env;
///
/// if env::is_ci() {
///     println!("No prompts in CI");
/// }
/// ```
#[must_use]
pub fn is_ci() -> bool {
    is_ci_from(|name| std::env::var_os(name))
}

/// CI systems that do not set `CI`.
const CI_VENDOR_VARS: [&str; 4] = [
    "TF_BUILD",
    "JENKINS_URL",
    "TEAMCITY_VERSION",
    "CODEBUILD_BUILD_ID",
];

fn is_ci_from(get: impl Fn(&str) -> Option<OsString>) -> bool {
    let set = |name: &str| get(name).filter(|value| !value.is_empty());
    set("CI").map_or_else(
        || CI_VENDOR_VARS.iter().any(|name| set(name).is_some()),
        |value| !(value.eq_ignore_ascii_case("false") || value == "0"),
    )
}

/// Reads a variable. `None` if it is unset.
#[expect(
    clippy::unnecessary_debug_formatting,
    reason = "Debug shows the bytes that are not UTF-8"
)]
fn read(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(raw)) => {
            bail!("Environment variable {name} is not valid UTF-8: {raw:?}")
        }
    }
}

fn parse<T: FromStr>(name: &str, value: &str) -> Result<T>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    value
        .parse()
        .with_context(|| format!("Failed to parse environment variable {name}={value:?}"))
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
        .filter(|name| std::env::var_os(name).is_none())
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
    fn var_parse_error_names_variable_and_value() {
        unsafe { std::env::set_var("SCRIPTKIT_VAR_BAD", "808O") };
        let err = var::<u16>("SCRIPTKIT_VAR_BAD").unwrap_err();
        unsafe { std::env::remove_var("SCRIPTKIT_VAR_BAD") };
        let msg = format!("{err:#}");
        assert!(msg.contains("SCRIPTKIT_VAR_BAD"), "{msg}");
        assert!(msg.contains("\"808O\""), "{msg}");
    }

    #[test]
    fn var_reads_empty_value_as_is() {
        unsafe { std::env::set_var("SCRIPTKIT_VAR_EMPTY", "") };
        let value: String = var("SCRIPTKIT_VAR_EMPTY").unwrap();
        unsafe { std::env::remove_var("SCRIPTKIT_VAR_EMPTY") };
        assert_eq!(value, "");
    }

    #[test]
    fn var_or_returns_value_when_set() {
        unsafe { std::env::set_var("SCRIPTKIT_TEST_OR", "100") };
        let value: i32 = var_or("SCRIPTKIT_TEST_OR", 50).unwrap();
        assert_eq!(value, 100);
        unsafe { std::env::remove_var("SCRIPTKIT_TEST_OR") };
    }

    #[test]
    fn var_or_returns_default_when_missing() {
        unsafe { std::env::remove_var("SCRIPTKIT_MISSING_OR") };
        let value: i32 = var_or("SCRIPTKIT_MISSING_OR", 50).unwrap();
        assert_eq!(value, 50);
    }

    #[test]
    fn var_or_returns_default_when_empty() {
        unsafe { std::env::set_var("SCRIPTKIT_EMPTY_OR", "") };
        let value: u16 = var_or("SCRIPTKIT_EMPTY_OR", 8080).unwrap();
        let text: String = var_or("SCRIPTKIT_EMPTY_OR", "dflt".to_string()).unwrap();
        unsafe { std::env::remove_var("SCRIPTKIT_EMPTY_OR") };
        assert_eq!(value, 8080);
        assert_eq!(text, "dflt");
    }

    /// Regression: `PORT=808O` gave the default.
    #[test]
    fn var_or_errors_on_parse_error() {
        unsafe { std::env::set_var("SCRIPTKIT_BAD_OR", "808O") };
        let result: Result<u16> = var_or("SCRIPTKIT_BAD_OR", 8080);
        unsafe { std::env::remove_var("SCRIPTKIT_BAD_OR") };
        let msg = format!("{:#}", result.unwrap_err());
        assert!(msg.contains("SCRIPTKIT_BAD_OR"), "{msg}");
        assert!(msg.contains("\"808O\""), "{msg}");
    }

    #[test]
    fn var_opt_returns_some_when_set() {
        unsafe { std::env::set_var("SCRIPTKIT_OPT_SET", "42") };
        let value: Option<u16> = var_opt("SCRIPTKIT_OPT_SET").unwrap();
        unsafe { std::env::remove_var("SCRIPTKIT_OPT_SET") };
        assert_eq!(value, Some(42));
    }

    #[test]
    fn var_opt_returns_none_when_unset() {
        unsafe { std::env::remove_var("SCRIPTKIT_OPT_UNSET") };
        let value: Option<u16> = var_opt("SCRIPTKIT_OPT_UNSET").unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn var_opt_returns_none_when_empty() {
        unsafe { std::env::set_var("SCRIPTKIT_OPT_EMPTY", "") };
        let value: Option<String> = var_opt("SCRIPTKIT_OPT_EMPTY").unwrap();
        unsafe { std::env::remove_var("SCRIPTKIT_OPT_EMPTY") };
        assert_eq!(value, None);
    }

    #[test]
    fn var_opt_errors_on_parse_error() {
        unsafe { std::env::set_var("SCRIPTKIT_OPT_BAD", "808O") };
        let result: Result<Option<u16>> = var_opt("SCRIPTKIT_OPT_BAD");
        unsafe { std::env::remove_var("SCRIPTKIT_OPT_BAD") };
        let msg = format!("{:#}", result.unwrap_err());
        assert!(msg.contains("SCRIPTKIT_OPT_BAD"), "{msg}");
        assert!(msg.contains("\"808O\""), "{msg}");
    }

    /// Only an empty value is unset. Whitespace is a value.
    #[test]
    fn var_opt_errors_on_whitespace_value() {
        unsafe { std::env::set_var("SCRIPTKIT_OPT_SPACE", " ") };
        let result: Result<Option<u16>> = var_opt("SCRIPTKIT_OPT_SPACE");
        unsafe { std::env::remove_var("SCRIPTKIT_OPT_SPACE") };
        assert!(result.is_err());
    }

    /// `var_or(n, d)` is `var_opt(n)?.unwrap_or(d)` for each kind of value.
    #[test]
    fn var_or_matches_var_opt_with_default() {
        for value in [None, Some(""), Some(" "), Some("7"), Some("x"), Some("-1")] {
            match value {
                Some(v) => unsafe { std::env::set_var("SCRIPTKIT_OR_OPT", v) },
                None => unsafe { std::env::remove_var("SCRIPTKIT_OR_OPT") },
            }
            let or = var_or::<u8>("SCRIPTKIT_OR_OPT", 9).ok();
            let opt = var_opt::<u8>("SCRIPTKIT_OR_OPT")
                .ok()
                .map(|v| v.unwrap_or(9));
            assert_eq!(or, opt, "value {value:?}");
        }
        unsafe { std::env::remove_var("SCRIPTKIT_OR_OPT") };
    }

    #[cfg(unix)]
    fn set_non_utf8(name: &str) {
        use std::os::unix::ffi::OsStrExt;
        let raw = std::ffi::OsStr::from_bytes(b"80\xff80");
        unsafe { std::env::set_var(name, raw) };
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_value_is_an_error() {
        set_non_utf8("SCRIPTKIT_NON_UTF8");
        let results = [
            var::<String>("SCRIPTKIT_NON_UTF8").map(|_| ()),
            var_opt::<String>("SCRIPTKIT_NON_UTF8").map(|_| ()),
            var_or::<String>("SCRIPTKIT_NON_UTF8", String::new()).map(|_| ()),
        ];
        unsafe { std::env::remove_var("SCRIPTKIT_NON_UTF8") };
        for result in results {
            let msg = format!("{:#}", result.unwrap_err());
            assert!(msg.contains("SCRIPTKIT_NON_UTF8"), "{msg}");
            assert!(msg.contains("not valid UTF-8"), "{msg}");
        }
    }

    /// Regression: a non-UTF-8 value was "missing".
    #[cfg(unix)]
    #[test]
    fn require_vars_accepts_non_utf8_value() {
        set_non_utf8("SCRIPTKIT_REQ_NON_UTF8");
        let result = require_vars(&["SCRIPTKIT_REQ_NON_UTF8"]);
        unsafe { std::env::remove_var("SCRIPTKIT_REQ_NON_UTF8") };
        assert!(result.is_ok(), "{result:?}");
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

    fn lookup<'a>(
        vars: &'a [(&'a str, &'a str)],
    ) -> impl Fn(&str) -> Option<std::ffi::OsString> + 'a {
        move |name| {
            vars.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| std::ffi::OsString::from(v))
        }
    }

    #[test]
    fn is_ci_reads_ci_variable() {
        for value in ["true", "True", "1", "yes", "anything"] {
            assert!(is_ci_from(lookup(&[("CI", value)])), "CI={value:?}");
        }
        for value in ["false", "FALSE", "False", "0"] {
            assert!(!is_ci_from(lookup(&[("CI", value)])), "CI={value:?}");
        }
    }

    #[test]
    fn is_ci_is_false_with_no_variables() {
        assert!(!is_ci_from(lookup(&[])));
        assert!(!is_ci_from(lookup(&[("CI", "")])));
    }

    #[test]
    fn is_ci_reads_vendor_variables_when_ci_is_unset() {
        for name in CI_VENDOR_VARS {
            assert!(is_ci_from(lookup(&[(name, "x")])), "{name}");
            assert!(is_ci_from(lookup(&[("CI", ""), (name, "x")])), "{name}");
            assert!(!is_ci_from(lookup(&[(name, "")])), "{name} empty");
        }
    }

    #[test]
    fn is_ci_false_overrides_vendor_variables() {
        for name in CI_VENDOR_VARS {
            assert!(
                !is_ci_from(lookup(&[("CI", "false"), (name, "x")])),
                "{name}"
            );
            assert!(!is_ci_from(lookup(&[("CI", "0"), (name, "x")])), "{name}");
        }
    }

    /// Sets `CI` for one test. Restores the old value on drop.
    struct CiGuard {
        old: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    static CI_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    impl CiGuard {
        fn set(value: Option<&str>) -> Self {
            let lock = CI_LOCK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let old = std::env::var_os("CI");
            match value {
                Some(v) => unsafe { std::env::set_var("CI", v) },
                None => unsafe { std::env::remove_var("CI") },
            }
            Self { old, _lock: lock }
        }
    }

    impl Drop for CiGuard {
        fn drop(&mut self) {
            match &self.old {
                Some(v) => unsafe { std::env::set_var("CI", v) },
                None => unsafe { std::env::remove_var("CI") },
            }
        }
    }

    #[test]
    fn is_ci_reads_process_environment() {
        {
            let _ci = CiGuard::set(Some("true"));
            assert!(is_ci());
        }
        {
            let _ci = CiGuard::set(Some("false"));
            assert!(!is_ci());
        }
        {
            let _ci = CiGuard::set(None);
            let vendor = CI_VENDOR_VARS
                .iter()
                .any(|name| std::env::var_os(name).is_some_and(|v| !v.is_empty()));
            assert_eq!(is_ci(), vendor);
        }
    }
}
