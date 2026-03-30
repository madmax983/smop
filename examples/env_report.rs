//! Example: Load a script configuration from environment variables.
//!
//! Run with: `cargo run --example env_report`
//!
//! Optional environment variables:
//! - `SMOP_SCRIPT_NAME`
//! - `SMOP_PORT`
//! - `SMOP_RETRIES`
//! - `SMOP_OUTPUT_DIR`

use smop::prelude::*;
use std::fs as stdfs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, PartialEq, Eq)]
struct ScriptConfig {
    script_name: String,
    port: u16,
    retries: u8,
    output_dir: String,
    path_entries: usize,
}

fn main() -> Result<()> {
    env::dotenv()?;
    env::require_vars(&["PATH"])?;

    let config = load_config()?;
    let output_dir = PathBuf::from(&config.output_dir);

    stdfs::create_dir_all(&output_dir).with_context(|| {
        format!(
            "Failed to create output directory: {}",
            output_dir.display()
        )
    })?;

    let json_path = output_dir.join("env-report.json");
    let toml_path = output_dir.join("env-report.toml");

    fs::write_json(&json_path, &config)?;
    fs::write_toml(&toml_path, &config)?;

    println!("Script: {}", config.script_name);
    println!("PATH entries: {}", config.path_entries);
    println!("Wrote: {}", json_path.display());
    println!("Wrote: {}", toml_path.display());

    Ok(())
}

fn load_config() -> Result<ScriptConfig> {
    let path_value: String = env::var("PATH")?;
    let script_name = env::var_or("SMOP_SCRIPT_NAME", String::from("daily-report"));
    let port = env::var_or("SMOP_PORT", 8080);
    let retries = env::var_or("SMOP_RETRIES", 3);
    let output_dir = path::expand(env::var_or(
        "SMOP_OUTPUT_DIR",
        String::from("./target/smop-example-output/env-report"),
    ));

    Ok(build_config(
        &path_value,
        script_name,
        port,
        retries,
        &output_dir,
    ))
}

fn build_config(
    path_value: &str,
    script_name: String,
    port: u16,
    retries: u8,
    output_dir: &Path,
) -> ScriptConfig {
    ScriptConfig {
        script_name,
        port,
        retries,
        output_dir: output_dir.display().to_string(),
        path_entries: count_path_entries(path_value),
    }
}

fn count_path_entries(path_value: &str) -> usize {
    std::env::split_paths(path_value).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_path_entries_matches_joined_paths() {
        let raw_path = std::env::join_paths(["bin", "tools"]).unwrap();
        let raw_path = raw_path.to_string_lossy().into_owned();

        assert_eq!(count_path_entries(&raw_path), 2);
    }

    #[test]
    fn build_config_expands_output_directory() {
        let output_dir = path::expand("./target/example-output");
        let config = build_config("bin", "demo".to_string(), 8080, 3, &output_dir);

        assert_eq!(config.script_name, "demo");
        assert_eq!(config.port, 8080);
        assert_eq!(config.retries, 3);
        assert_eq!(PathBuf::from(&config.output_dir), output_dir);
        assert_eq!(config.path_entries, 1);
    }
}
