//! `changelog`: one file per change in `changelog.d/`, written into
//! `CHANGELOG.md` at a release. See `changelog.d/README.md`.
//!
//! A fragment is `changelog.d/<name>.<section>.md`. Its text is one or more
//! Markdown list items.

use std::path::Path;

use smop::prelude::*;

/// Keep a Changelog sections, in release order. The first is smop's own.
pub const SECTIONS: [&str; 7] = [
    "breaking",
    "added",
    "changed",
    "deprecated",
    "removed",
    "fixed",
    "security",
];
const SKIP: [&str; 2] = ["README.md", ".gitkeep"];
const REPO: &str = "https://github.com/madmax983/smop";
/// A released version: `## [0.3.2] - 2026-11-03`. "Unreleased" does not match.
const RELEASED: &str = r"(?m)^## \[([^\]]+)\] - \d{4}-";
/// A version link line: `[0.3.2]: <url>`. Other link lines do not match.
const LINK: &str = r"(?m)^\[\d[^\]]*\]: ";

/// One file in `changelog.d/`.
#[derive(Debug, PartialEq, Eq)]
pub struct Fragment {
    /// `None` when the file name is not `<name>.<section>.md`.
    pub section: Option<&'static str>,
    pub name: String,
    pub text: String,
}

/// The fragments in `dir`, sorted by file name.
pub fn fragments(dir: &Path) -> Result<Vec<Fragment>> {
    let names = sorted_names(dir)?;
    let pattern = re::compile(&format!(
        r"^[a-z0-9]+(?:-[a-z0-9]+)*\.({})\.md$",
        SECTIONS.join("|")
    ))?;
    let mut out = Vec::new();
    for name in names {
        if SKIP.contains(&name.as_str()) {
            continue;
        }
        let text = fs::read_string(dir.join(&name))?;
        let section = pattern
            .captures(&name)
            .and_then(|c| SECTIONS.iter().copied().find(|s| *s == &c[1]));
        out.push(Fragment {
            section,
            name,
            text,
        });
    }
    Ok(out)
}

fn sorted_names(dir: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("Failed to read directory: {}", dir.display()))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

/// The problems with `changelog.d/` and `CHANGELOG.md` under `root`. Empty
/// means all is correct.
pub fn check(root: &Path) -> Result<Vec<String>> {
    let mut errors = Vec::new();
    for fragment in fragments(&root.join("changelog.d"))? {
        let name = &fragment.name;
        if fragment.section.is_none() {
            errors.push(format!(
                "changelog.d/{name}: name must be <name>.<section>.md, with a lowercase name and a section from: {}",
                SECTIONS.join(", ")
            ));
        } else if !fragment.text.starts_with("- ") {
            errors.push(format!(
                "changelog.d/{name}: text must start with a list item ('- ')"
            ));
        }
    }
    let text = fs::read_string(root.join("CHANGELOG.md"))?;
    if re::is_match(r"(?m)^## .*Unreleased", &text)? {
        errors.push(
            "CHANGELOG.md: has an Unreleased section. Put unreleased changes in changelog.d/"
                .into(),
        );
    }
    for version in released(&text)? {
        let link = format!(r"(?m)^\[{}\]: \S", re::escape(&version));
        if !re::is_match(&link, &text)? {
            errors.push(format!(
                "CHANGELOG.md: [{version}] has no link line ([{version}]: <url>)"
            ));
        }
    }
    Ok(errors)
}

/// The released versions in `CHANGELOG.md` text, newest first.
fn released(text: &str) -> Result<Vec<String>> {
    Ok(re::compile(RELEASED)?
        .captures_iter(text)
        .map(|c| c[1].to_string())
        .collect())
}

/// The link line for a release: a compare link, or the tag for the first.
pub fn link(version: &str, previous: Option<&str>) -> String {
    previous.map_or_else(
        || format!("[{version}]: {REPO}/releases/tag/v{version}"),
        |previous| format!("[{version}]: {REPO}/compare/v{previous}...v{version}"),
    )
}

/// The release notes for `items` under `heading`, or `""` if there are none.
pub fn render(items: &[Fragment], heading: &str) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut parts = vec![format!("{heading}\n")];
    for section in SECTIONS {
        let mut texts = String::new();
        for item in items.iter().filter(|f| f.section == Some(section)) {
            texts.push_str(item.text.trim_end_matches('\n'));
            texts.push('\n');
        }
        if !texts.is_empty() {
            let mut title = section.to_string();
            title[..1].make_ascii_uppercase();
            parts.push(format!("### {title}\n{texts}"));
        }
    }
    parts.join("\n")
}

/// Writes the fragments into `CHANGELOG.md` as release `version` of `date`
/// (`YYYY-MM-DD`), then removes them. Changes nothing on an error.
pub fn release(root: &Path, version: &str, date: &str) -> Result<()> {
    let mut errors = check(root)?;
    if time::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
        errors.push(format!("date {date}: must be YYYY-MM-DD"));
    }
    if !errors.is_empty() {
        bail!(errors.join("\n"));
    }
    let dir = root.join("changelog.d");
    let items = fragments(&dir)?;
    ensure!(!items.is_empty(), "changelog.d has no fragments to release");
    let path = root.join("CHANGELOG.md");
    let mut text = fs::read_string(&path)?;
    let versions = released(&text)?;
    ensure!(
        !versions.iter().any(|v| v == version),
        "CHANGELOG.md: {version} is released"
    );

    let notes = render(&items, &format!("## [{version}] - {date}")) + "\n";
    let line = link(version, versions.first().map(String::as_str));
    let at = text.find("\n## [").map_or(text.len(), |i| i + 1);
    text.insert_str(at, &notes);
    match re::compile(LINK)?.find(&text).map(|m| m.start()) {
        Some(first_link) => text.insert_str(first_link, &format!("{line}\n")),
        None => text = format!("{}\n\n{line}\n", text.trim_end_matches('\n')),
    }
    fs::write_string(&path, text)?;
    for item in items {
        fs::remove(dir.join(item.name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "# Changelog\n\nIntro.\n\n";
    const OLD: &str = "## [0.3.2] - 2026-11-03\n\n### Added\n- Old entry.\n";
    const LINKS: &str = "\n[0.3.2]: https://github.com/madmax983/smop/compare/v0.3.1...v0.3.2\n";

    /// A temporary root with `CHANGELOG.md` and `changelog.d/`.
    struct Repo(tempfile::TempDir);

    impl Repo {
        fn new(changelog: &str) -> Self {
            let repo = Self(tempfile::tempdir().unwrap());
            std::fs::create_dir(repo.dir()).unwrap();
            repo.write("CHANGELOG.md", changelog);
            repo.write("changelog.d/README.md", "# How to\n");
            repo
        }

        fn standard() -> Self {
            Self::new(&format!("{HEAD}{OLD}{LINKS}"))
        }

        fn root(&self) -> &Path {
            self.0.path()
        }

        fn dir(&self) -> std::path::PathBuf {
            self.root().join("changelog.d")
        }

        fn write(&self, rel: &str, text: &str) {
            std::fs::write(self.root().join(rel), text).unwrap();
        }

        fn read(&self, rel: &str) -> String {
            std::fs::read_to_string(self.root().join(rel)).unwrap()
        }

        fn fragment(&self, name: &str, text: &str) {
            self.write(&format!("changelog.d/{name}"), text);
        }

        fn names(&self) -> Vec<String> {
            sorted_names(&self.dir()).unwrap()
        }
    }

    #[test]
    fn render_puts_sections_in_fixed_order_and_files_by_name() {
        let repo = Repo::standard();
        repo.fragment("zeta.fixed.md", "- Fix Z.\n");
        repo.fragment("b.added.md", "- Add B.\n  More text.\n");
        repo.fragment("a.added.md", "- Add A.\n");
        repo.fragment("x.breaking.md", "- Break X.\n");
        let text = render(&fragments(&repo.dir()).unwrap(), "## [Unreleased]");
        assert_eq!(
            text,
            "## [Unreleased]\n\n\
             ### Breaking\n- Break X.\n\n\
             ### Added\n- Add A.\n- Add B.\n  More text.\n\n\
             ### Fixed\n- Fix Z.\n"
        );
    }

    #[test]
    fn render_of_no_fragments_is_empty() {
        assert_eq!(render(&[], "## [Unreleased]"), "");
    }

    #[test]
    fn good_fragments_pass() {
        let repo = Repo::standard();
        repo.fragment("env-var-or-2.breaking.md", "- Text.\n");
        assert_eq!(check(repo.root()).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn bad_names_fail() {
        for name in [
            "Upper.added.md",
            "x.feature.md",
            "x.md",
            "x.added.txt",
            "x..added.md",
        ] {
            let repo = Repo::standard();
            repo.fragment(name, "- Text.\n");
            let errors = check(repo.root()).unwrap();
            assert_eq!(errors.len(), 1, "{name}");
            assert!(errors[0].contains(name), "{name}: {errors:?}");
        }
    }

    #[test]
    fn content_must_be_a_list_item() {
        for text in ["", "\n", "Text without a bullet.\n", "## Heading\n"] {
            let repo = Repo::standard();
            repo.fragment("x.added.md", text);
            let errors = check(repo.root()).unwrap();
            assert_eq!(errors.len(), 1, "{text:?}");
            assert!(errors[0].contains("x.added.md"));
        }
    }

    #[test]
    fn an_unreleased_section_in_the_changelog_fails() {
        let repo = Repo::new(&format!("{HEAD}## [0.4.0] - Unreleased\n\n{OLD}{LINKS}"));
        let errors = check(repo.root()).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Unreleased"));
    }

    #[test]
    fn a_release_without_a_link_fails() {
        let repo = Repo::new(&format!("{HEAD}{OLD}"));
        let errors = check(repo.root()).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("[0.3.2]"));
    }

    #[test]
    fn the_link_of_another_version_does_not_count() {
        let repo = Repo::new(&format!("{HEAD}{OLD}\n[0.3.20]: https://example.com\n"));
        assert_eq!(check(repo.root()).unwrap().len(), 1);
    }

    #[test]
    fn release_writes_the_section_and_removes_the_fragments() {
        let repo = Repo::standard();
        repo.fragment("a.added.md", "- Add A.\n");
        repo.fragment("b.changed.md", "- Change B.\n");
        release(repo.root(), "0.4.0", "2026-12-01").unwrap();
        assert_eq!(
            repo.read("CHANGELOG.md"),
            format!(
                "{HEAD}## [0.4.0] - 2026-12-01\n\n### Added\n- Add A.\n\n### Changed\n- Change B.\n\n{OLD}\
                 \n[0.4.0]: https://github.com/madmax983/smop/compare/v0.3.2...v0.4.0{LINKS}"
            )
        );
        assert_eq!(check(repo.root()).unwrap(), Vec::<String>::new());
        assert_eq!(repo.names(), ["README.md"]);
    }

    #[test]
    fn the_first_release_links_to_its_tag() {
        let repo = Repo::new(HEAD);
        repo.fragment("a.added.md", "- Add A.\n");
        release(repo.root(), "0.1.0", "2026-12-01").unwrap();
        assert_eq!(
            repo.read("CHANGELOG.md"),
            format!(
                "{HEAD}## [0.1.0] - 2026-12-01\n\n### Added\n- Add A.\n\
                 \n[0.1.0]: https://github.com/madmax983/smop/releases/tag/v0.1.0\n"
            )
        );
    }

    #[test]
    fn the_link_goes_to_the_version_links_not_another_link() {
        let repo = Repo::new(&format!(
            "{HEAD}{OLD}\nSee [RFC].\n\n[RFC]: https://example.com\n{LINKS}"
        ));
        repo.fragment("a.added.md", "- Add A.\n");
        release(repo.root(), "0.4.0", "2026-12-01").unwrap();
        assert!(repo.read("CHANGELOG.md").contains(
            "[RFC]: https://example.com\n\n[0.4.0]: https://github.com/madmax983/smop/compare/v0.3.2...v0.4.0\n[0.3.2]: "
        ));
    }

    fn assert_release_fails(repo: &Repo, version: &str, date: &str, message: &str) {
        let before = repo.read("CHANGELOG.md");
        let names = repo.names();
        let err = release(repo.root(), version, date).unwrap_err().to_string();
        assert!(err.contains(message), "{err}");
        assert_eq!(repo.read("CHANGELOG.md"), before);
        assert_eq!(repo.names(), names);
    }

    #[test]
    fn a_release_with_a_bad_date_changes_nothing() {
        let repo = Repo::standard();
        repo.fragment("a.added.md", "- Add A.\n");
        assert_release_fails(&repo, "0.4.0", "12/01/2026", "12/01/2026");
    }

    #[test]
    fn a_release_of_a_released_version_changes_nothing() {
        let repo = Repo::standard();
        repo.fragment("a.added.md", "- Add A.\n");
        assert_release_fails(&repo, "0.3.2", "2026-12-01", "0.3.2");
    }

    #[test]
    fn a_release_without_fragments_fails() {
        let repo = Repo::standard();
        assert_release_fails(&repo, "0.4.0", "2026-12-01", "no fragments");
    }

    #[test]
    fn a_release_with_a_bad_fragment_changes_nothing() {
        let repo = Repo::standard();
        repo.fragment("a.added.md", "- Add A.\n");
        repo.fragment("b.nope.md", "- Bad.\n");
        assert_release_fails(&repo, "0.4.0", "2026-12-01", "b.nope.md");
        assert!(repo.names().contains(&"a.added.md".to_string()));
    }

    #[test]
    fn the_repository_changelog_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        assert_eq!(check(&root).unwrap(), Vec::<String>::new());
    }
}
