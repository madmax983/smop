//! Reads a script configuration from environment variables. Writes it as
//! JSON and TOML.
//!
//! ```sh
//! SMOP_PORT=9000 cargo run --example env_report
//! SMOP_PORT=90x0 cargo run --example env_report   # error: bad value
//! ```
//!
//! All variables are optional: `SMOP_SCRIPT_NAME`, `SMOP_PORT`,
//! `SMOP_RETRIES`, `SMOP_LOG_LEVEL`, `SMOP_OUTPUT_DIR`. An empty value is
//! the same as unset. A bad value is an error, not the default. A `.env`
//! file in the current directory also sets them.

use std::path::PathBuf;

use smop::prelude::*;

/// The configuration of the script.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(crate = "smop::serde")]
struct ScriptConfig {
    script_name: String,
    port: u16,
    retries: u8,
    log_level: Option<String>,
    ci: bool,
    output_dir: String,
    path_entries: usize,
}

fn main() -> Result<()> {
    env::dotenv()?;
    env::require_vars(&["PATH"])?;

    let config = load_config()?;
    let dir = PathBuf::from(&config.output_dir);
    fs::create_dir_all(&dir)?;

    let json = dir.join("env-report.json");
    let toml = dir.join("env-report.toml");
    fs::write_json(&json, &config)?;
    fs::write_toml(&toml, &config)?;

    println!("Script: {} (port {})", config.script_name, config.port);
    println!("PATH entries: {}", config.path_entries);
    println!("Wrote: {}", json.display());
    println!("Wrote: {}", toml.display());
    Ok(())
}

/// Reads the configuration. Each bad value is an error that names the
/// variable.
fn load_config() -> Result<ScriptConfig> {
    let path: String = env::var("PATH")?;
    let output_dir: String = env::var_or(
        "SMOP_OUTPUT_DIR",
        "target/smop-example-output/env-report".to_string(),
    )?;
    Ok(ScriptConfig {
        script_name: env::var_or("SMOP_SCRIPT_NAME", "daily-report".to_string())?,
        port: env::var_or("SMOP_PORT", 8080)?,
        retries: env::var_or("SMOP_RETRIES", 3)?,
        log_level: env::var_opt("SMOP_LOG_LEVEL")?,
        ci: env::is_ci(),
        output_dir: expand_dir(&output_dir),
        path_entries: count_path_entries(&path),
    })
}

/// Expands `~` and `$VAR` in `dir`.
fn expand_dir(dir: &str) -> String {
    path::expand(dir).display().to_string()
}

/// Counts the directories in a `PATH` value.
fn count_path_entries(path: &str) -> usize {
    std::env::split_paths(path).count()
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn count_path_entries_counts_each_directory() {
        let joined = std::env::join_paths(["bin", "tools", "sbin"]).unwrap();
        assert_eq!(count_path_entries(&joined.to_string_lossy()), 3);
    }

    #[test]
    fn expand_dir_expands_home() {
        let home = path::home();
        assert_eq!(
            PathBuf::from(expand_dir("~/out")),
            home.join("out"),
            "~ is the home directory"
        );
        assert_eq!(expand_dir("target/out"), "target/out");
    }

    // The only test that changes the environment, so tests do not race.
    #[test]
    fn load_config_reads_defaults_and_rejects_a_bad_value() {
        let names = [
            "SMOP_SCRIPT_NAME",
            "SMOP_PORT",
            "SMOP_RETRIES",
            "SMOP_LOG_LEVEL",
            "SMOP_OUTPUT_DIR",
        ];
        for name in names {
            // SAFETY: no other test in this binary reads or writes these names.
            unsafe { std::env::remove_var(name) };
        }

        let config = load_config().unwrap();
        assert_eq!(config.script_name, "daily-report");
        assert_eq!(config.port, 8080);
        assert_eq!(config.retries, 3);
        assert_eq!(config.log_level, None);
        assert!(Path::new(&config.output_dir).ends_with("env-report"));

        unsafe { std::env::set_var("SMOP_PORT", "") };
        assert_eq!(load_config().unwrap().port, 8080, "empty is unset");

        unsafe { std::env::set_var("SMOP_PORT", "90x0") };
        let err = format!("{:#}", load_config().unwrap_err());
        assert!(err.contains("SMOP_PORT"), "{err}");

        unsafe { std::env::remove_var("SMOP_PORT") };
    }
}
