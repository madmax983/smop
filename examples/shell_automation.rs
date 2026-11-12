//! Reports on the Cargo workspace: the tool versions and the packages from
//! `cargo metadata`. Writes the report as JSON in the target directory.
//!
//! ```sh
//! cargo run --example shell_automation
//! ```
//!
//! When a command fails, the example exits with the code of that command.

use std::path::PathBuf;

use smop::prelude::*;

/// The part of the `cargo metadata` output that the report uses.
#[derive(Debug, Deserialize)]
#[serde(crate = "smop::serde")]
struct Metadata {
    packages: Vec<Package>,
    target_directory: PathBuf,
    workspace_root: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(crate = "smop::serde")]
struct Package {
    name: String,
    version: String,
}

/// The report that the example writes.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(crate = "smop::serde")]
struct Report {
    cargo: String,
    rustc: String,
    workspace_root: PathBuf,
    packages: Vec<String>,
}

fn main() {
    script::main(|| {
        // sh::cargo() runs $CARGO when cargo starts the example: the same
        // cargo as the outer `cargo run`.
        let cargo_version = sh::cargo().arg("--version").output()?;
        let rustc_version = cmd!("rustc --version").output()?;
        let raw = script::timed("cargo metadata", || {
            sh::cargo()
                .args(["metadata", "--format-version", "1", "--no-deps"])
                .output()
        })?;
        let metadata = parse_metadata(&raw)?;
        let report = build_report(cargo_version, rustc_version, &metadata);

        let path = metadata
            .target_directory
            .join("smop-example-output/shell-automation.json");
        fs::create_dir_all(path.parent().context("no parent directory")?)?;
        fs::write_json(&path, &report)?;

        println!("{}", report.cargo);
        println!("{}", report.rustc);
        for package in &report.packages {
            println!("package: {package}");
        }
        println!("Wrote: {}", path.display());
        Ok(())
    })
}

/// Parses the JSON from `cargo metadata --format-version 1`.
fn parse_metadata(raw: &str) -> Result<Metadata> {
    smop::serde_json::from_str(raw).context("Failed to parse the output of cargo metadata")
}

/// Makes the report. Packages are `name version`, sorted.
fn build_report(cargo: String, rustc: String, metadata: &Metadata) -> Report {
    let mut packages: Vec<String> = metadata
        .packages
        .iter()
        .map(|p| format!("{} {}", p.name, p.version))
        .collect();
    packages.sort();
    Report {
        cargo,
        rustc,
        workspace_root: metadata.workspace_root.clone(),
        packages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const METADATA: &str = r#"{
        "packages": [
            {"name": "smop-macros", "version": "0.4.0", "id": "x"},
            {"name": "smop", "version": "0.4.0", "id": "y"}
        ],
        "target_directory": "/work/target",
        "workspace_root": "/work",
        "version": 1
    }"#;

    #[test]
    fn parse_metadata_reads_packages_and_directories() {
        let metadata = parse_metadata(METADATA).unwrap();
        assert_eq!(metadata.packages.len(), 2);
        assert_eq!(metadata.target_directory, PathBuf::from("/work/target"));
        assert_eq!(metadata.workspace_root, PathBuf::from("/work"));
    }

    #[test]
    fn parse_metadata_error_names_the_command() {
        let err = format!("{:#}", parse_metadata("not json").unwrap_err());
        assert!(err.contains("cargo metadata"), "{err}");
    }

    #[test]
    fn build_report_sorts_packages() {
        let metadata = parse_metadata(METADATA).unwrap();
        let report = build_report("cargo 1.97.0".into(), "rustc 1.97.0".into(), &metadata);
        assert_eq!(
            report,
            Report {
                cargo: "cargo 1.97.0".into(),
                rustc: "rustc 1.97.0".into(),
                workspace_root: PathBuf::from("/work"),
                packages: vec!["smop 0.4.0".into(), "smop-macros 0.4.0".into()],
            }
        );
    }
}
