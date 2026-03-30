//! Example: Inspect the current Cargo workspace with shell commands.
//!
//! Run with: `cargo run --example shell_automation`

use smop::prelude::*;
use std::fs as stdfs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    target_directory: String,
    workspace_root: String,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: String,
    version: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct ToolchainReport {
    cargo_path: String,
    cargo_version: String,
    rustc_version: String,
    workspace_root: String,
    target_directory: String,
    package_count: usize,
    first_package: Option<String>,
}

fn main() -> Result<()> {
    let workspace = path::cwd()?;
    let cargo_path = path::which("cargo")?;
    let cargo_version = sh::cmd(&cargo_path).arg("--version").output()?;
    let rustc_version = sh::output("rustc --version")?;
    let metadata = read_metadata(&workspace, &cargo_path)?;
    let report = build_report(&cargo_path, &cargo_version, &rustc_version, &metadata);

    let output_dir = path::expand("./target/smop-example-output/shell-automation");
    stdfs::create_dir_all(&output_dir).with_context(|| {
        format!(
            "Failed to create output directory: {}",
            output_dir.display()
        )
    })?;

    let report_path = output_dir.join("toolchain-report.json");
    fs::write_json(&report_path, &report)?;

    println!("Cargo: {}", report.cargo_version);
    println!("Rustc: {}", report.rustc_version);
    println!("Packages: {}", report.package_count);
    if let Some(package) = &report.first_package {
        println!("First package: {package}");
    }
    println!("Wrote: {}", report_path.display());

    Ok(())
}

fn read_metadata(workspace: &Path, cargo_path: &Path) -> Result<CargoMetadata> {
    let raw = sh::cmd(cargo_path)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .dir(workspace)
        .env("CARGO_TERM_COLOR", "never")
        .output()?;

    serde_json::from_str(&raw).context("Failed to parse `cargo metadata` output")
}

fn build_report(
    cargo_path: &Path,
    cargo_version: &str,
    rustc_version: &str,
    metadata: &CargoMetadata,
) -> ToolchainReport {
    ToolchainReport {
        cargo_path: cargo_path.display().to_string(),
        cargo_version: cargo_version.to_string(),
        rustc_version: rustc_version.to_string(),
        workspace_root: metadata.workspace_root.clone(),
        target_directory: metadata.target_directory.clone(),
        package_count: metadata.packages.len(),
        first_package: metadata
            .packages
            .first()
            .map(|package| format!("{} {}", package.name, package.version)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_report_summarizes_metadata() {
        let metadata = CargoMetadata {
            packages: vec![
                CargoPackage {
                    name: "smop".to_string(),
                    version: "0.2.1".to_string(),
                },
                CargoPackage {
                    name: "helper".to_string(),
                    version: "0.1.0".to_string(),
                },
            ],
            target_directory: "target".to_string(),
            workspace_root: "/workspace".to_string(),
        };

        let report = build_report(
            Path::new("/usr/bin/cargo"),
            "cargo 1.88.0",
            "rustc 1.88.0",
            &metadata,
        );

        assert_eq!(report.package_count, 2);
        assert_eq!(report.workspace_root, "/workspace");
        assert_eq!(report.target_directory, "target");
        assert_eq!(report.first_package.as_deref(), Some("smop 0.2.1"));
    }
}
