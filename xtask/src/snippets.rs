//! `snippets`: compile every ```` ```rust ```` block in README.md and
//! CLAUDE.md against this checkout (all features). Compile-only: snippets
//! use the network and the file system.
//!
//! Prints one line per block: `PASS`, `FAIL` (a defect) or `CTX` (the block
//! names a user-side item such as `User` or `url`: illustrative, counted but
//! not a failure), then the counts.

use std::path::{Path, PathBuf};

use smop::prelude::*;

/// One ```` ```rust ```` block.
#[derive(Debug, PartialEq, Eq)]
pub struct Snippet {
    /// The binary name: file stem in lowercase letters, then the line.
    pub id: String,
    pub file: String,
    /// The line of the first line of code.
    pub line: usize,
    /// The program: the block, in a `main` with the prelude if it has none.
    pub source: String,
}

/// The ```` ```rust ```` blocks of `text`. A block that starts with `#!`
/// (cargo-script frontmatter, which needs nightly) is left out.
pub fn extract(file: &str, text: &str) -> Vec<Snippet> {
    let stem: String = Path::new(file)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_lowercase)
        .collect();
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let opens = lines[i].strip_prefix("```rust").is_some_and(|rest| {
            rest.chars()
                .next()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
        });
        if !opens {
            i += 1;
            continue;
        }
        let start = i + 1;
        let end = lines[start..]
            .iter()
            .position(|l| l.starts_with("```"))
            .map_or(lines.len(), |n| start + n);
        let body = lines[start..end].join("\n");
        i = end + 1;
        if body.starts_with("#!") {
            continue;
        }
        let has_main = re::is_match(r"\bfn main\b", &body).unwrap_or(false);
        let source = if has_main {
            body
        } else {
            format!("use smop::prelude::*;\nfn main() -> Result<()> {{\n{body}\nOk(())\n}}\n")
        };
        out.push(Snippet {
            id: format!("{stem}_{start}"),
            file: file.to_string(),
            line: start,
            source: format!("#![allow(unused)]\n{source}\n"),
        });
    }
    out
}

/// How a block that does not compile fails.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    /// Each error is an undefined name (E0425, E0412, E0422): a fragment
    /// that names a user-side item.
    Context,
    /// Any other error.
    Defect,
}

/// Classifies `cargo check --message-format short` output of a failed block.
pub fn classify(output: &str) -> Failure {
    let errors = output
        .lines()
        .filter(|l| l.starts_with("src") && l.contains("error"))
        .count();
    let undefined = output
        .lines()
        .filter(|l| {
            ["error[E0425]", "error[E0412]", "error[E0422]"]
                .iter()
                .any(|e| l.contains(e))
        })
        .count();
    if errors > 0 && errors == undefined {
        Failure::Context
    } else {
        Failure::Defect
    }
}

/// The scratch crate for the snippets of `files`, in `work`. Its own
/// `[workspace]` keeps it out of smop's workspace.
fn write_crate(work: &Path, smop: &Path, snippets: &[Snippet]) -> Result<()> {
    let bins = work.join("src/bin");
    fs::remove_if_exists(&bins)?;
    fs::create_dir_all(&bins)?;
    let manifest = format!(
        "[package]\nname = \"snippets\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n\
         [dependencies]\nsmop = {{ path = {smop:?} }}\n\n[workspace]\n",
        smop = smop.display().to_string()
    );
    fs::write_string(work.join("Cargo.toml"), manifest)?;
    for snippet in snippets {
        fs::write_string(bins.join(format!("{}.rs", snippet.id)), &snippet.source)?;
    }
    Ok(())
}

/// Compiles the snippets of README.md and CLAUDE.md under `root`. An error
/// when more than `max_broken` are defects.
pub fn run(root: &Path, max_broken: usize) -> Result<()> {
    let mut snippets = Vec::new();
    for file in ["README.md", "CLAUDE.md"] {
        snippets.extend(extract(file, &fs::read_string(root.join(file))?));
    }
    let work: PathBuf = root.join("target/xtask/snippets");
    write_crate(&work, root, &snippets)?;
    sh::cargo()
        .args(["fetch", "--quiet"])
        .dir(&work)
        .quiet()
        .run()?;

    let (mut broken, mut context) = (0, 0);
    for snippet in &snippets {
        let place = format!("{}:{}", snippet.file, snippet.line);
        let result = sh::cargo()
            .args([
                "check",
                "--quiet",
                "--offline",
                "--message-format",
                "short",
                "--bin",
                &snippet.id,
            ])
            .dir(&work)
            .stderr_to_stdout()
            .capture()?;
        if result.success() {
            println!("PASS {place}");
            continue;
        }
        let out = result.stdout();
        let kind = match classify(out) {
            Failure::Context => {
                context += 1;
                "CTX"
            }
            Failure::Defect => {
                broken += 1;
                "FAIL"
            }
        };
        let first = out.lines().find(|l| l.contains("error")).unwrap_or("");
        println!("{kind} {place}  {first}");
    }
    println!(
        "snippets: total={} broken={broken} needs_context={context}",
        snippets.len()
    );
    ensure!(
        broken <= max_broken,
        "snippets: {broken} broken (allowed: {max_broken})"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# T\n\n```rust\nlet x = 1;\n```\n\n```rust,no_run\nfn main() {}\n```\n\n\
                       ```rustc\nnot rust\n```\n\n```rust\n#!/usr/bin/env cargo\n```\n\n```toml\na = 1\n```\n";

    #[test]
    fn extract_wraps_fragments_and_keeps_programs() {
        let found = extract("README.md", DOC);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, "readme_3");
        assert_eq!(found[0].line, 3);
        assert_eq!(
            found[0].source,
            "#![allow(unused)]\nuse smop::prelude::*;\nfn main() -> Result<()> {\nlet x = 1;\nOk(())\n}\n\n"
        );
        assert_eq!(found[1].id, "readme_7");
        assert_eq!(found[1].source, "#![allow(unused)]\nfn main() {}\n");
    }

    #[test]
    fn the_id_uses_only_lowercase_letters_of_the_stem() {
        let found = extract("docs/My-File_2.md", "```rust\nfn main() {}\n```\n");
        assert_eq!(found[0].id, "myfile_1");
        assert_eq!(found[0].file, "docs/My-File_2.md");
    }

    #[test]
    fn an_unclosed_block_runs_to_the_end() {
        let found = extract("a.md", "```rust\nlet a = 1;");
        assert_eq!(found.len(), 1);
        assert!(found[0].source.contains("let a = 1;"));
    }

    #[test]
    fn undefined_user_names_are_context() {
        let out = "src/bin/readme_3.rs:4:9: error[E0425]: cannot find value `url`\n\
                   src/bin/readme_3.rs:5:9: error[E0412]: cannot find type `User`\nerror: could not compile";
        assert_eq!(classify(out), Failure::Context);
    }

    #[test]
    fn other_errors_are_defects() {
        let out = "src/bin/readme_3.rs:4:9: error[E0425]: cannot find value `url`\n\
                   src/bin/readme_3.rs:6:9: error[E0599]: no method named `nope`";
        assert_eq!(classify(out), Failure::Defect);
        assert_eq!(classify("error: linking with `cc` failed"), Failure::Defect);
    }

    #[test]
    fn the_scratch_crate_is_its_own_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let snippets = extract("README.md", DOC);
        write_crate(dir.path(), Path::new("/smop"), &snippets).unwrap();
        let manifest = fs::read_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(
            manifest.contains("smop = { path = \"/smop\" }"),
            "{manifest}"
        );
        assert!(manifest.ends_with("[workspace]\n"));
        assert!(dir.path().join("src/bin/readme_3.rs").is_file());
        assert!(dir.path().join("src/bin/readme_7.rs").is_file());
    }
}
