//! `ci-features` and `docs-features`: smop's features, for CI and for
//! CLAUDE.md's feature table.

use std::collections::{BTreeMap, BTreeSet};

use smop::prelude::*;

use crate::meta::Package;

/// smop's features without `default` and `full`, sorted.
///
/// CI's per-feature test and clippy loops read this list, so a new feature
/// is tested and linted on its own without editing the workflow. No
/// features is an error, so a broken lookup can't silently skip every
/// per-feature check.
pub fn ci_features(smop: &Package) -> Result<Vec<&str>> {
    let features: Vec<&str> = smop
        .features
        .keys()
        .map(String::as_str)
        .filter(|f| !matches!(*f, "default" | "full"))
        .collect();
    ensure!(
        !features.is_empty(),
        "cargo metadata lists no features for smop"
    );
    Ok(features)
}

/// Drift between the features and the table under `## Features` in
/// `claude_md`. Each feature but `default` needs a row. A row names each
/// `dep:<crate>` its feature enables, and no optional crate that the
/// feature does not enable, directly or through other features.
pub fn table_drift(
    features: &BTreeMap<String, Vec<String>>,
    claude_md: &str,
) -> Result<Vec<String>> {
    let deps = |f: &str| -> BTreeSet<&str> {
        features
            .get(f)
            .into_iter()
            .flatten()
            .filter_map(|v| v.strip_prefix("dep:"))
            .collect()
    };
    let optional: BTreeSet<&str> = features.keys().flat_map(|f| deps(f)).collect();

    let lines: Vec<&str> = claude_md.lines().collect();
    let start = lines
        .iter()
        .position(|l| *l == "## Features")
        .context("CLAUDE.md has no `## Features` section")?;
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.starts_with("## "))
        .map_or(lines.len(), |i| start + 1 + i);

    let row = re::compile(r"^\| `([\w-]+)`\s*\| (.*?)\s*\|")?;
    let word = re::compile(r"[\w-]+")?;
    let mut drift = Vec::new();
    let mut rows = BTreeSet::new();
    for (n, line) in lines.iter().enumerate().take(end).skip(start) {
        let Some(c) = row.captures(line) else {
            continue;
        };
        let feature = &c[1];
        if !features.contains_key(feature) {
            continue;
        }
        rows.insert(feature.to_string());
        let named: BTreeSet<&str> = word
            .find_iter(c.get(2).map_or("", |m| m.as_str()))
            .map(|m| m.as_str())
            .filter(|t| optional.contains(t))
            .collect();
        let line_no = n + 1;
        for dep in deps(feature).difference(&named) {
            drift.push(format!(
                "CLAUDE.md:{line_no} `{feature}` row omits dep {dep}"
            ));
        }
        let reached = reach(features, feature);
        for dep in named.iter().filter(|d| !reached.contains(**d)) {
            drift.push(format!(
                "CLAUDE.md:{line_no} `{feature}` row names {dep}, not enabled by feature"
            ));
        }
    }
    for feature in features.keys() {
        if feature != "default" && !rows.contains(feature) {
            drift.push(format!("CLAUDE.md: no row for feature `{feature}`"));
        }
    }
    Ok(drift)
}

/// The optional crates that `feature` enables, also through other features.
fn reach<'a>(features: &'a BTreeMap<String, Vec<String>>, feature: &str) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::new();
    let mut crates = BTreeSet::new();
    let mut todo = vec![feature.to_string()];
    while let Some(f) = todo.pop() {
        let Some(values) = features.get(&f) else {
            continue;
        };
        if !seen.insert(f) {
            continue;
        }
        for value in values {
            match value.strip_prefix("dep:") {
                Some(dep) => {
                    crates.insert(dep);
                }
                None => todo.push(value.clone()),
            }
        }
    }
    crates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::Package;

    fn features(list: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        list.iter()
            .map(|(f, v)| {
                (
                    (*f).to_string(),
                    v.iter().map(|s| (*s).to_string()).collect(),
                )
            })
            .collect()
    }

    fn package(list: &[(&str, &[&str])]) -> Package {
        Package {
            name: "smop".into(),
            manifest_path: "Cargo.toml".into(),
            features: features(list),
            targets: Vec::new(),
        }
    }

    const FEATURES: &[(&str, &[&str])] = &[
        ("default", &["full"]),
        ("full", &["fs", "json"]),
        ("fs", &["dep:tempfile"]),
        ("serde", &["dep:serde"]),
        ("json", &["fs", "serde", "dep:serde_json"]),
        ("sh", &[]),
    ];

    fn doc(rows: &str) -> String {
        format!(
            "# x\n\n## Features\n\n| Feature | Dependencies | What |\n|--|--|--|\n{rows}\n## Next\n\n| `sh` | serde | outside the section |\n"
        )
    }

    const GOOD: &str = "| `sh` | (none) | sh |\n| `fs` | tempfile | fs |\n| `serde` | serde | x |\n\
                        | `json` | serde, serde_json (+fs, so tempfile) | json |\n| `full` | All | all |\n";

    #[test]
    fn ci_features_leave_out_default_and_full() {
        assert_eq!(
            ci_features(&package(FEATURES)).unwrap(),
            ["fs", "json", "serde", "sh"]
        );
    }

    #[test]
    fn ci_features_fail_when_there_are_none() {
        assert!(ci_features(&package(&[("default", &[])])).is_err());
    }

    #[test]
    fn a_matching_table_has_no_drift() {
        assert_eq!(
            table_drift(&features(FEATURES), &doc(GOOD)).unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_missing_row_an_omitted_dep_and_a_foreign_dep_are_drift() {
        let rows = "| `fs` | (none) | fs |\n| `serde` | serde, tempfile | x |\n\
                    | `json` | serde, serde_json | json |\n| `full` | All | all |\n";
        assert_eq!(
            table_drift(&features(FEATURES), &doc(rows)).unwrap(),
            [
                "CLAUDE.md:7 `fs` row omits dep tempfile",
                "CLAUDE.md:8 `serde` row names tempfile, not enabled by feature",
                "CLAUDE.md: no row for feature `sh`",
            ]
        );
    }

    #[test]
    fn a_dep_reached_through_another_feature_may_be_named() {
        let json_names_tempfile = "| `json` | serde, serde_json, tempfile | json |";
        assert!(GOOD.contains("(+fs, so tempfile)"));
        let rows = GOOD.replace(
            "| `json` | serde, serde_json (+fs, so tempfile) | json |",
            json_names_tempfile,
        );
        assert!(
            table_drift(&features(FEATURES), &doc(&rows))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn no_features_section_is_an_error() {
        assert!(table_drift(&features(FEATURES), "# x\n").is_err());
    }

    #[test]
    fn reach_survives_a_cycle() {
        let cyclic = features(&[("a", &["b", "dep:x"]), ("b", &["a", "dep:y"])]);
        assert_eq!(reach(&cyclic, "a"), BTreeSet::from(["x", "y"]));
    }
}
