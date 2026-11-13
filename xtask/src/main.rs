//! smop's own checks and release steps, written with smop (ADR 0017).
//!
//! Run `cargo xtask <task>` from anywhere in the repository; `cargo xtask
//! --help` lists the tasks. CI runs the same commands.

mod api_boundary;
mod changelog;
mod clean_room;
mod drift;
mod examples;
mod features;
mod meta;
mod snippets;
mod tag_release;

use std::path::{Path, PathBuf};

use smop::prelude::*;

use crate::meta::{Metadata, Scope};

/// smop's checks and release steps.
#[derive(Debug, Parser)]
#[command(name = "cargo xtask", version = None)]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Debug, Subcommand)]
enum Task {
    /// Check, preview or release the fragments in changelog.d/.
    Changelog {
        #[command(subcommand)]
        action: ChangelogAction,
    },
    /// Print smop's features, one per line, without `default` and `full`.
    CiFeatures,
    /// Find types of dependencies in smop's public API (rustdoc).
    ApiBoundary,
    /// Check each `module::symbol` that the docs name, and ADR numbers.
    DocsDrift,
    /// Check CLAUDE.md's feature table against Cargo.toml.
    DocsFeatures,
    /// Build each example with only its required-features.
    Examples,
    /// Compile each Rust block of README.md and CLAUDE.md.
    Snippets {
        /// Fail only when more blocks than this are broken.
        #[arg(long, default_value_t = 0)]
        max_broken: usize,
    },
    /// Follow the README's first run in a new crate, to `cargo run`.
    CleanRoom {
        /// Use this smop checkout instead of crates.io.
        #[arg(long)]
        smop_path: Option<PathBuf>,
        /// The README to follow (default: this repository's).
        #[arg(long)]
        readme: Option<PathBuf>,
    },
    /// Tag published versions on the commit that crates.io records.
    TagRelease {
        /// Versions, such as 0.4.0.
        #[arg(required = true)]
        versions: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ChangelogAction {
    /// Check the fragments and CHANGELOG.md (CI runs this).
    Check,
    /// Print the next release notes.
    Preview,
    /// Write the fragments into CHANGELOG.md and remove them.
    Release {
        /// The version, such as 0.5.0.
        version: String,
        /// The date, YYYY-MM-DD (default: today).
        date: Option<String>,
    },
}

fn main() {
    script::main(|| run(Cli::parse().task, &path::workspace_root()?))
}

/// Runs `task` in the workspace at `root`.
fn run(task: Task, root: &Path) -> Result<()> {
    match task {
        Task::Changelog { action } => changelog_task(root, action),
        Task::CiFeatures => {
            let meta = Metadata::load(root, Scope::Workspace)?;
            for feature in features::ci_features(meta.package("smop")?)? {
                println!("{feature}");
            }
            Ok(())
        }
        Task::ApiBoundary => api_boundary::run(root),
        Task::DocsDrift => drift::run(root),
        Task::DocsFeatures => {
            let meta = Metadata::load(root, Scope::Workspace)?;
            let claude = fs::read_string(root.join("CLAUDE.md"))?;
            report_table(&meta.package("smop")?.features, &claude)
        }
        Task::Examples => examples::run(root),
        Task::Snippets { max_broken } => snippets::run(root, max_broken),
        Task::CleanRoom { smop_path, readme } => {
            let readme = readme.unwrap_or_else(|| root.join("README.md"));
            clean_room::run(&readme, smop_path.as_deref())
        }
        Task::TagRelease { versions } => tag_release::run(root, &versions),
    }
}

/// Prints the drift of CLAUDE.md's feature table; an error if there is some.
fn report_table(
    features: &std::collections::BTreeMap<String, Vec<String>>,
    claude: &str,
) -> Result<()> {
    let drift = features::table_drift(features, claude)?;
    println!("feature-table drift: {}", drift.len());
    for line in &drift {
        println!("DRIFT {line}");
    }
    ensure!(
        drift.is_empty(),
        "feature table: {} drifted rows",
        drift.len()
    );
    Ok(())
}

fn changelog_task(root: &Path, action: ChangelogAction) -> Result<()> {
    match action {
        ChangelogAction::Check => {
            let errors = changelog::check(root)?;
            ensure!(errors.is_empty(), errors.join("\n"));
            println!("changelog: ok");
        }
        ChangelogAction::Preview => {
            let items = changelog::fragments(&root.join("changelog.d"))?;
            let notes = changelog::render(&items, "## [Unreleased]");
            println!(
                "{}",
                if notes.is_empty() {
                    "changelog.d has no fragments"
                } else {
                    &notes
                }
            );
        }
        ChangelogAction::Release { version, date } => {
            let date = date.unwrap_or_else(|| time::format(&time::now_local(), "%Y-%m-%d"));
            changelog::release(root, &version, &date)?;
            println!("CHANGELOG.md: added {version}. Commit CHANGELOG.md and changelog.d/.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cli_is_valid() {
        use smop::cli::clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn tasks_parse() {
        let cli = Cli::try_parse_from(["cargo xtask", "snippets", "--max-broken", "2"]).unwrap();
        assert!(matches!(cli.task, Task::Snippets { max_broken: 2 }));
        let cli = Cli::try_parse_from(["cargo xtask", "changelog", "release", "0.5.0"]).unwrap();
        assert!(matches!(
            cli.task,
            Task::Changelog {
                action: ChangelogAction::Release { date: None, .. }
            }
        ));
        assert!(Cli::try_parse_from(["cargo xtask", "tag-release"]).is_err());
    }

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    #[test]
    fn the_quick_tasks_pass_on_the_repository() {
        for task in [
            Task::CiFeatures,
            Task::DocsFeatures,
            Task::DocsDrift,
            Task::Changelog {
                action: ChangelogAction::Check,
            },
            Task::Changelog {
                action: ChangelogAction::Preview,
            },
        ] {
            let name = format!("{task:?}");
            run(task, &root()).unwrap_or_else(|e| panic!("{name}: {e:#}"));
        }
    }

    #[test]
    fn report_table_fails_on_a_drifted_table() {
        let meta = Metadata::load(&root(), Scope::Workspace).unwrap();
        let features = &meta.package("smop").unwrap().features;
        let claude = fs::read_string(root().join("CLAUDE.md")).unwrap();
        assert!(report_table(features, &claude).is_ok());
        let drifted = claude.replace("| `hash`  |", "| `hashes` |");
        let err = report_table(features, &drifted).unwrap_err();
        assert_eq!(err.to_string(), "feature table: 1 drifted rows");
    }

    #[test]
    fn changelog_release_writes_the_date_given() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("changelog.d")).unwrap();
        std::fs::write(dir.path().join("CHANGELOG.md"), "# Changelog\n\n").unwrap();
        std::fs::write(dir.path().join("changelog.d/a.added.md"), "- A.\n").unwrap();
        let release = ChangelogAction::Release {
            version: "0.1.0".into(),
            date: Some("2026-01-02".into()),
        };
        changelog_task(dir.path(), release).unwrap();
        let text = fs::read_string(dir.path().join("CHANGELOG.md")).unwrap();
        assert!(text.contains("## [0.1.0] - 2026-01-02"), "{text}");
        assert!(changelog_task(dir.path(), ChangelogAction::Check).is_ok());
        std::fs::write(dir.path().join("changelog.d/Bad.md"), "x").unwrap();
        assert!(changelog_task(dir.path(), ChangelogAction::Check).is_err());
    }
}
