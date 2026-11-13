//! `examples`: each file in `examples/` is an `[[example]]` target with
//! `required-features`, builds with only those features (so the list is
//! complete) and has a `cargo run --example <name>` line in README.md. Then
//! `cargo build --examples --all-features` builds all of them.

use std::path::Path;

use smop::prelude::*;

use crate::meta::{Metadata, Scope, Target};

/// The example targets of smop.
pub fn example_targets(targets: &[Target]) -> Vec<&Target> {
    targets.iter().filter(|t| t.kind == ["example"]).collect()
}

/// The problems that need no build: a file with no target, a target with
/// no `required-features`, a target that README.md does not run.
/// `files` are the stems of the `.rs` files in `examples/`.
pub fn static_errors(files: &[String], targets: &[&Target], readme: &str) -> Result<Vec<String>> {
    let mut errors = Vec::new();
    for file in files {
        if !targets.iter().any(|t| &t.name == file) {
            errors.push(format!("examples/{file}.rs: no [[example]] target"));
        }
    }
    for target in targets {
        let name = &target.name;
        if target.required_features.is_empty() {
            errors.push(format!("example {name}: no required-features"));
        }
        let run = format!(r"(?m)cargo run --example {}( |`|$)", re::escape(name));
        if !re::is_match(&run, readme)? {
            errors.push(format!("example {name}: not in README.md"));
        }
    }
    Ok(errors)
}

/// The stems of the `.rs` files directly in `dir`, sorted.
pub fn example_files(dir: &Path) -> Result<Vec<String>> {
    let mut files: Vec<String> = fs::walk(dir)?
        .iter()
        .filter(|p| p.parent() == Some(dir) && p.extension().is_some_and(|e| e == "rs"))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    files.sort();
    Ok(files)
}

/// Runs the check in `root`. Prints a GitHub `::error::` line for each
/// problem; an error if there is one.
pub fn run(root: &Path) -> Result<()> {
    let meta = Metadata::load(root, Scope::Workspace)?;
    let targets = example_targets(&meta.package("smop")?.targets);
    let files = example_files(&root.join("examples"))?;
    let readme = fs::read_string(root.join("README.md"))?;
    let mut errors = static_errors(&files, &targets, &readme)?;

    for target in targets.iter().filter(|t| !t.required_features.is_empty()) {
        let (name, features) = (&target.name, target.required_features.join(","));
        println!("::group::example {name} (features: {features})");
        let built = sh::cargo()
            .args([
                "build",
                "-p",
                "smop",
                "--no-default-features",
                "--features",
                &features,
                "--example",
                name,
            ])
            .dir(root)
            .status()?
            .success();
        println!("::endgroup::");
        if !built {
            errors.push(format!(
                "example {name}: does not build with only {features} ({})",
                target.src_path.display()
            ));
        }
    }
    if !sh::cargo()
        .args(["build", "-p", "smop", "--examples", "--all-features"])
        .dir(root)
        .status()?
        .success()
    {
        errors.push("cargo build --examples failed".into());
    }

    for error in &errors {
        println!("::error::{error}");
    }
    ensure!(errors.is_empty(), "examples: {} problems", errors.len());
    println!("examples: {} ok", targets.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(name: &str, kind: &str, features: &[&str]) -> Target {
        Target {
            name: name.into(),
            kind: vec![kind.into()],
            src_path: format!("examples/{name}.rs").into(),
            required_features: features.iter().map(|f| (*f).to_string()).collect(),
        }
    }

    #[test]
    fn only_example_targets_count() {
        let all = [target("smop", "lib", &[]), target("a", "example", &["sh"])];
        let examples = example_targets(&all);
        assert_eq!(examples.len(), 1);
        assert_eq!(examples[0].name, "a");
    }

    #[test]
    fn a_complete_example_has_no_errors() {
        let a = target("a", "example", &["sh"]);
        let readme = "Run `cargo run --example a`.\n";
        assert_eq!(
            static_errors(&["a".into()], &[&a], readme).unwrap(),
            Vec::<String>::new()
        );
        let at_line_end = "cargo run --example a\n";
        assert_eq!(
            static_errors(&["a".into()], &[&a], at_line_end).unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn each_kind_of_problem_is_reported() {
        let (a, b) = (target("a", "example", &[]), target("b", "example", &["fs"]));
        let readme = "cargo run --example bb\n";
        assert_eq!(
            static_errors(&["a".into(), "c".into()], &[&a, &b], readme).unwrap(),
            [
                "examples/c.rs: no [[example]] target",
                "example a: no required-features",
                "example a: not in README.md",
                "example b: not in README.md",
            ]
        );
    }

    #[test]
    fn the_repository_examples_pass_the_static_checks() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let meta = Metadata::load(&root, Scope::Workspace).unwrap();
        let targets = example_targets(&meta.package("smop").unwrap().targets);
        let names: Vec<String> = targets.iter().map(|t| t.name.clone()).collect();
        let readme = fs::read_string(root.join("README.md")).unwrap();
        assert_eq!(
            static_errors(&names, &targets, &readme).unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn example_files_are_the_rs_files_at_the_top() {
        let dir = tempfile::tempdir().unwrap();
        for file in ["b.rs", "a.rs", "notes.md", "sub/c.rs"] {
            fs::write_with_parents(dir.path().join(file), "").unwrap();
        }
        assert_eq!(example_files(dir.path()).unwrap(), ["a", "b"]);
    }
}
