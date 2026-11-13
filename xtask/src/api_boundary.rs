//! `api-boundary`: smop's public API has no types of crates that smop does
//! not re-export on purpose (see "API boundaries" in CLAUDE.md).
//!
//! Rustdoc links each type to the docs of its crate. This task documents
//! smop and all its dependencies, direct and transitive. Then it reads
//! smop's pages and finds links to crates that are not on the allow list.
//!
//! Not checked: `#[doc(hidden)]` items and the expansion of macros.

use std::path::{Component, Path, PathBuf};

use smop::prelude::*;

use crate::meta::{Metadata, Scope};

/// Re-exported on purpose: their major releases are smop major releases.
/// chrono: only the date types that `time` re-exports. regex: all of it, as
/// `smop::re`. The other names are internal crates of the allowed crates.
pub const ALLOWED: [&str; 11] = [
    "anyhow",
    "clap",
    "clap_builder",
    "clap_derive",
    "serde",
    "serde_core",
    "serde_derive",
    "serde_json",
    "toml",
    "chrono",
    "regex",
];

/// smop itself and the standard library.
const OWN: [&str; 5] = ["smop", "std", "core", "alloc", "proc_macro"];

fn ok(krate: &str) -> bool {
    ALLOWED.contains(&krate) || OWN.contains(&krate)
}

/// Lexical normalization, as Python's `os.path.normpath`: no file system.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// The crate that `link` on a page in `dir` points to, or `None`. `doc` is
/// the root of the rustdoc output.
pub fn crate_of(link: &str, dir: &Path, doc: &Path) -> Option<String> {
    if let Some(rest) = link.strip_prefix("https://docs.rs/") {
        return rest.split('/').next().map(|c| c.replace('-', "_"));
    }
    if link.starts_with("https://doc.rust-lang.org/") {
        return Some("std".into());
    }
    if link.contains("://") || link.starts_with("mailto:") {
        return None;
    }
    let target = normalize(&dir.join(link));
    let relative = target.strip_prefix(normalize(doc)).ok()?;
    let mut parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if parts.first().is_some_and(|p| p == "src") {
        parts.remove(0);
    }
    let first = parts.first()?;
    let is_name = !first.is_empty()
        && first
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    (parts.len() >= 2 && is_name).then(|| first.clone())
}

/// The boundary problems of one rustdoc page. `shown` names the page in the
/// report, `dir` is its directory and `doc` the root of the output.
pub fn page_hits(page: &str, shown: &str, dir: &Path, doc: &Path) -> Result<Vec<String>> {
    // A page for an item that smop re-exports from another crate.
    let src = re::captures(r##"<a class="src[^"]*" href="([^"#]+)"##, page)?;
    let owner = src.map_or_else(|| Some("smop".into()), |c| crate_of(&c[1], dir, doc));
    if let Some(owner) = &owner {
        if ALLOWED.contains(&owner.as_str()) {
            return Ok(Vec::new());
        }
        if !ok(owner) {
            return Ok(vec![format!("{shown}: re-exports an item of {owner}")]);
        }
    }
    // Implementor lists and blanket impls show other crates' types, not smop's API.
    let cut = re::compile(r#"id="(?:foreign-impls|implementors|blanket-implementations)""#)?;
    let own = cut.split(page).next().unwrap_or(page);
    let links: std::collections::BTreeSet<&str> = re::compile(r##"href="([^"#]+)"##)?
        .captures_iter(own)
        .filter_map(|c| c.get(1).map(|m| m.as_str()))
        .collect();
    Ok(links
        .into_iter()
        .filter_map(|link| {
            crate_of(link, dir, doc)
                .filter(|krate| !ok(krate))
                .map(|krate| format!("{shown}: {krate} ({link})"))
        })
        .collect())
}

/// The boundary problems of every page under `doc/smop`, sorted.
pub fn scan(doc: &Path) -> Result<Vec<String>> {
    let mut hits = Vec::new();
    for path in fs::walk(doc.join("smop"))? {
        if path.extension().is_none_or(|e| e != "html") {
            continue;
        }
        let page = fs::read_string(&path)?;
        let dir = path.parent().context("a page has no directory")?;
        let shown = path
            .strip_prefix(doc)
            .unwrap_or(&path)
            .display()
            .to_string();
        hits.extend(page_hits(&page, &shown, dir, doc)?);
    }
    hits.sort();
    Ok(hits)
}

/// Documents smop, then checks its pages. Prints the report; an error on a hit.
pub fn run(root: &Path) -> Result<()> {
    let target = Metadata::load(root, Scope::Workspace)?.target_directory;
    let doc = target.join("doc");
    // Rustdoc links to a crate only if the docs of that crate exist when
    // smop's docs start. Cargo documents crates in parallel, so document all
    // crates first, then document smop again on its own.
    sh::cargo()
        .args(["doc", "-p", "smop", "--all-features", "--quiet"])
        .dir(root)
        .run()?;
    fs::remove_if_exists(doc.join("smop"))?;
    fs::remove_if_exists(doc.join("src/smop"))?;
    sh::cargo()
        .args([
            "doc",
            "-p",
            "smop",
            "--no-deps",
            "--all-features",
            "--quiet",
        ])
        .dir(root)
        .run()?;

    let hits = scan(&doc)?;
    if hits.is_empty() {
        println!("api boundary: ok");
        return Ok(());
    }
    println!("Types of other crates in smop's public API:");
    for hit in &hits {
        println!("{hit}");
    }
    println!("A link in a doc comment to the docs of a crate also shows here.");
    bail!("api boundary: {} hits", hits.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> PathBuf {
        PathBuf::from("/t/doc")
    }

    fn page_dir() -> PathBuf {
        doc().join("smop/fs")
    }

    #[test]
    fn crate_of_reads_relative_links() {
        let (dir, doc) = (page_dir(), doc());
        assert_eq!(
            crate_of("../../tempfile/struct.TempDir.html", &dir, &doc).as_deref(),
            Some("tempfile")
        );
        assert_eq!(
            crate_of("struct.TempDir.html", &dir, &doc).as_deref(),
            Some("smop")
        );
        assert_eq!(
            crate_of("../../src/zip/lib.rs.html", &dir, &doc).as_deref(),
            Some("zip")
        );
        assert_eq!(
            crate_of("./../index.html", &dir, &doc).as_deref(),
            Some("smop")
        );
    }

    #[test]
    fn crate_of_reads_absolute_links() {
        let (dir, doc) = (page_dir(), doc());
        assert_eq!(
            crate_of(
                "https://docs.rs/comfy-table/8/comfy_table/struct.Table.html",
                &dir,
                &doc
            )
            .as_deref(),
            Some("comfy_table")
        );
        assert_eq!(
            crate_of(
                "https://doc.rust-lang.org/nightly/std/path/struct.Path.html",
                &dir,
                &doc
            )
            .as_deref(),
            Some("std")
        );
        assert_eq!(crate_of("https://example.com/a/b", &dir, &doc), None);
        assert_eq!(crate_of("mailto:me@example.com", &dir, &doc), None);
    }

    #[test]
    fn crate_of_skips_pages_that_are_not_in_a_crate() {
        let (dir, doc) = (page_dir(), doc());
        assert_eq!(crate_of("../../help.html", &dir, &doc), None);
        assert_eq!(crate_of("../../static.files/x.css", &dir, &doc), None);
        assert_eq!(crate_of("../../../../outside/a.html", &dir, &doc), None);
        assert_eq!(crate_of("../../Upper/a.html", &dir, &doc), None);
    }

    #[test]
    fn a_page_with_a_foreign_type_is_a_hit() {
        let page = r##"<a class="src" href="../../src/smop/fs.rs.html#1">source</a>
            <a href="../../tempfile/struct.TempDir.html">TempDir</a>
            <a href="../../anyhow/type.Result.html">Result</a>
            <a href="#method.x">x</a>"##;
        let hits = page_hits(page, "smop/fs/fn.x.html", &page_dir(), &doc()).unwrap();
        assert_eq!(
            hits,
            ["smop/fs/fn.x.html: tempfile (../../tempfile/struct.TempDir.html)"]
        );
    }

    #[test]
    fn implementor_lists_do_not_count() {
        let page = r#"<a href="struct.Own.html">Own</a>
            <h2 id="implementors">Implementors</h2><a href="../../zip/struct.X.html">X</a>"#;
        assert!(
            page_hits(page, "p", &page_dir(), &doc())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_reexport_of_an_allowed_crate_is_skipped_and_others_are_hits() {
        let regex = r#"<a class="src" href="../../src/regex/lib.rs.html">src</a><a href="../../zip/a.html">a</a>"#;
        assert!(
            page_hits(regex, "p", &page_dir(), &doc())
                .unwrap()
                .is_empty()
        );
        let zip = r#"<a class="src" href="../../src/zip/lib.rs.html">src</a>"#;
        assert_eq!(
            page_hits(zip, "p", &page_dir(), &doc()).unwrap(),
            ["p: re-exports an item of zip"]
        );
    }

    #[test]
    fn scan_reads_each_html_page_under_smop() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path();
        let pages = [
            (
                "smop/fs/fn.a.html",
                r#"<a href="../../tempfile/struct.T.html">T</a>"#,
            ),
            (
                "smop/fs/fn.b.html",
                r#"<a href="../../anyhow/type.Result.html">R</a>"#,
            ),
            (
                "smop/fs/notes.txt",
                r#"<a href="../../zip/struct.Z.html">Z</a>"#,
            ),
            (
                "other/index.html",
                r#"<a href="../zip/struct.Z.html">Z</a>"#,
            ),
        ];
        for (path, text) in pages {
            fs::write_with_parents(doc.join(path), text).unwrap();
        }
        let shown = Path::new("smop").join("fs").join("fn.a.html");
        assert_eq!(
            scan(doc).unwrap(),
            [format!(
                "{}: tempfile (../../tempfile/struct.T.html)",
                shown.display()
            )]
        );
    }
}
