//! `cargo metadata`, read into the few fields the tasks use.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use smop::prelude::*;

/// The output of `cargo metadata --format-version 1`.
#[derive(Debug, Deserialize)]
#[serde(crate = "smop::serde")]
pub struct Metadata {
    pub packages: Vec<Package>,
    pub target_directory: PathBuf,
}

/// One package of [`Metadata`].
#[derive(Debug, Deserialize)]
#[serde(crate = "smop::serde")]
pub struct Package {
    pub name: String,
    pub manifest_path: PathBuf,
    /// Feature name to what it enables: `dep:<crate>` or another feature.
    pub features: BTreeMap<String, Vec<String>>,
    pub targets: Vec<Target>,
}

/// One build target of a [`Package`].
#[derive(Debug, Deserialize)]
#[serde(crate = "smop::serde")]
pub struct Target {
    pub name: String,
    pub kind: Vec<String>,
    pub src_path: PathBuf,
    #[serde(rename = "required-features", default)]
    pub required_features: Vec<String>,
}

/// Which packages `cargo metadata` describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The workspace members only (`--no-deps`).
    Workspace,
    /// The members and every dependency, with all features on.
    AllDependencies,
}

impl Metadata {
    /// Runs `cargo metadata` in `root`.
    pub fn load(root: &Path, scope: Scope) -> Result<Self> {
        let flag = match scope {
            Scope::Workspace => "--no-deps",
            Scope::AllDependencies => "--all-features",
        };
        let json = sh::cargo()
            .args(["metadata", "--format-version", "1", flag])
            .dir(root)
            .quiet()
            .output()?;
        Self::parse(&json)
    }

    /// Parses the JSON that `cargo metadata` prints.
    pub fn parse(json: &str) -> Result<Self> {
        smop::serde_json::from_str(json).context("cargo metadata printed JSON that does not parse")
    }

    /// The first package named `name`.
    pub fn package(&self, name: &str) -> Result<&Package> {
        self.packages
            .iter()
            .find(|p| p.name == name)
            .with_context(|| format!("cargo metadata has no package {name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{
        "packages": [{
            "name": "smop",
            "manifest_path": "/r/Cargo.toml",
            "features": {"default": ["full"], "fs": ["dep:tempfile"]},
            "targets": [
                {"name": "smop", "kind": ["lib"], "src_path": "/r/src/lib.rs"},
                {"name": "xtask", "kind": ["example"], "src_path": "/r/examples/xtask.rs",
                 "required-features": ["sh", "fs"]}
            ]
        }],
        "target_directory": "/r/target"
    }"#;

    #[test]
    fn parses_the_fields_the_tasks_use() {
        let meta = Metadata::parse(JSON).unwrap();
        assert_eq!(meta.target_directory, PathBuf::from("/r/target"));
        let smop = meta.package("smop").unwrap();
        assert_eq!(smop.features["fs"], ["dep:tempfile"]);
        assert!(smop.targets[0].required_features.is_empty());
        assert_eq!(smop.targets[1].required_features, ["sh", "fs"]);
    }

    #[test]
    fn a_missing_package_is_an_error_that_names_it() {
        let meta = Metadata::parse(JSON).unwrap();
        let err = meta.package("nope").unwrap_err();
        assert_eq!(err.to_string(), "cargo metadata has no package nope");
    }

    #[test]
    fn bad_json_is_an_error() {
        assert!(Metadata::parse("{").is_err());
    }
}
