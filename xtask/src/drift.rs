//! `docs-drift`: every `module::symbol` that README.md, CLAUDE.md or an ADR
//! names must exist. Also: no two ADRs share a number, and the crate counts
//! that the docs quote, measured.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use smop::prelude::*;

use crate::meta::{Metadata, Scope};

/// The modules whose `module::symbol` names are checked.
pub const MODULES: [&str; 12] = [
    "path", "env", "fs", "sh", "http", "print", "time", "archive", "re", "hash", "git", "script",
];

/// The `module::name` uses in `text`: (line number, name). A use preceded by
/// `:` or a word character (`smop::fs::x`, `xfs::y`) is not one.
pub fn uses(text: &str, module: &str) -> Result<Vec<(usize, String)>> {
    let pattern = re::compile(&format!(r"{module}::([a-z_][a-z0-9_]*)"))?;
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        for c in pattern.captures_iter(line) {
            let Some(whole) = c.get(0) else { continue };
            let before = line[..whole.start()].chars().next_back();
            if before.is_some_and(|ch| ch == ':' || ch == '_' || ch.is_alphanumeric()) {
                continue;
            }
            out.push((n + 1, c[1].to_string()));
        }
    }
    Ok(out)
}

/// Whether `src` defines a public item `name`, or re-exports one.
pub fn defines(src: &str, name: &str) -> Result<bool> {
    let name = re::escape(name);
    re::is_match(
        &format!(r"pub (const )?(fn|struct|enum|mod|type|trait) {name}\b|pub use .*\b{name}\b"),
        src,
    )
}

/// Whether `src` defines a public item `name` (re-exports do not count).
fn defines_item(src: &str, name: &str) -> Result<bool> {
    let name = re::escape(name);
    re::is_match(
        &format!(r"pub (const )?(fn|struct|enum|mod|type|trait) {name}\b"),
        src,
    )
}

/// The crate that a module re-exports whole: `pub use regex::*;`.
pub fn glob_reexport(src: &str) -> Result<Option<String>> {
    Ok(re::captures(r"(?m)^pub use (\w+)::\*;", src)?.map(|c| c[1].to_string()))
}

/// ADR numbers that two or more files in `names` share.
pub fn duplicate_numbers<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen = BTreeMap::<String, usize>::new();
    for name in names {
        let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            *seen.entry(digits).or_default() += 1;
        }
    }
    seen.into_iter()
        .filter(|(_, n)| *n > 1)
        .map(|(d, _)| d)
        .collect()
}

/// The number of crates in `cargo tree --prefix none` output, smop not counted.
pub fn count_crates(tree: &str) -> usize {
    tree.lines()
        .map(|l| l.trim_end_matches(" (*)"))
        .filter(|l| !l.is_empty() && !l.starts_with("smop "))
        .collect::<BTreeSet<_>>()
        .len()
}

/// The source texts that a module's `module::name` uses are checked against.
struct Sources {
    module: String,
    /// The text of each file under the `src/` of a crate that the module
    /// re-exports whole.
    glob: Vec<String>,
}

/// Runs the scan in `root`. Prints the report; an error on drift.
pub fn run(root: &Path) -> Result<()> {
    let mut docs = vec![root.join("README.md"), root.join("CLAUDE.md")];
    let adr_dir = root.join("docs/adr");
    let adrs: Vec<PathBuf> = fs::walk(&adr_dir)?
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    docs.extend(adrs.iter().cloned());

    let mut deps = None;
    let mut sources = BTreeMap::new();
    for module in MODULES {
        let module_src = fs::read_string(root.join(format!("src/{module}.rs")))?;
        let mut glob = Vec::new();
        if let Some(krate) = glob_reexport(&module_src)? {
            if deps.is_none() {
                deps = Some(Metadata::load(root, Scope::AllDependencies)?);
            }
            if let Some(meta) = &deps {
                let manifest = &meta.package(&krate)?.manifest_path;
                let dir = manifest
                    .parent()
                    .context("manifest path has no parent")?
                    .join("src");
                for file in fs::walk(dir)? {
                    glob.push(String::from_utf8_lossy(&fs::read_bytes(file)?).into_owned());
                }
            }
        }
        sources.insert(
            module,
            Sources {
                module: module_src,
                glob,
            },
        );
    }

    let mut drift = BTreeSet::new();
    for doc in &docs {
        let text = fs::read_string(doc)?;
        let shown = doc.strip_prefix(root).unwrap_or(doc).display().to_string();
        for (module, src) in &sources {
            for (line, name) in uses(&text, module)? {
                if defines(&src.module, &name)? {
                    continue;
                }
                let mut found = false;
                for file in &src.glob {
                    if defines_item(file, &name)? {
                        found = true;
                        break;
                    }
                }
                if !found {
                    drift.insert(format!("DRIFT {shown}:{line} {module}::{name}"));
                }
            }
        }
    }

    let names: Vec<String> = adrs
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    let shared = duplicate_numbers(names.iter().map(String::as_str));
    println!(
        "duplicate ADR numbers: {} {}",
        shared.len(),
        shared.join(" ")
    );
    println!("symbol drift: {}", drift.len());
    for line in &drift {
        println!("{line}");
    }
    let tree = |features: &[&str]| -> Result<usize> {
        let out = sh::cargo()
            .args(["tree", "-p", "smop", "-e", "normal", "--prefix", "none"])
            .args(features)
            .dir(root)
            .quiet()
            .output()?;
        Ok(count_crates(&out))
    };
    println!(
        "crates: sh,fs,path={} (docs: 12)  full={} (docs: ~140)  http={} (docs: ~40)",
        tree(&["--no-default-features", "--features", "sh,fs,path"])?,
        tree(&[])?,
        tree(&["--no-default-features", "--features", "http"])?,
    );
    ensure!(
        drift.is_empty() && shared.is_empty(),
        "docs drift: {} symbols, {} duplicate ADR numbers",
        drift.len(),
        shared.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_skip_paths_and_longer_words() {
        let text = "Call `fs::read_string` and fs::walk.\nNot smop::fs::copy, xfs::a or my_fs::b.\n(fs::lock)";
        let found = uses(text, "fs").unwrap();
        assert_eq!(
            found,
            [
                (1, "read_string".to_string()),
                (1, "walk".to_string()),
                (3, "lock".to_string())
            ]
        );
    }

    #[test]
    fn uses_need_a_lowercase_name() {
        assert_eq!(
            uses("fs::TempDir and fs::{a, b}", "fs").unwrap(),
            Vec::<(usize, String)>::new()
        );
    }

    #[test]
    fn defines_finds_items_and_reexports() {
        let src = "pub fn walk() {}\npub struct TempDir;\npub const fn f() {}\npub use crate::x::{lock, other};\nfn private() {}";
        for name in ["walk", "TempDir", "f", "lock", "other"] {
            assert!(defines(src, name).unwrap(), "{name}");
        }
        assert!(!defines(src, "private").unwrap());
        assert!(!defines(src, "wal").unwrap());
    }

    #[test]
    fn defines_item_does_not_count_reexports() {
        assert!(!defines_item("pub use a::b;", "b").unwrap());
        assert!(defines_item("pub fn b() {}", "b").unwrap());
    }

    #[test]
    fn glob_reexport_finds_the_crate() {
        assert_eq!(
            glob_reexport("//! re\npub use regex::*;\n")
                .unwrap()
                .as_deref(),
            Some("regex")
        );
        assert_eq!(glob_reexport("pub use regex::Regex;\n").unwrap(), None);
    }

    #[test]
    fn duplicate_numbers_are_reported_once() {
        let names = [
            "0001-a.md",
            "0002-b.md",
            "0002-c.md",
            "0002-d.md",
            "README.md",
        ];
        assert_eq!(duplicate_numbers(names), ["0002"]);
        assert_eq!(
            duplicate_numbers(["0001-a.md", "0002-b.md"]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn count_crates_dedupes_and_leaves_out_smop() {
        let tree = "smop v0.4.0 (/r)\nanyhow v1.0.0\ntempfile v3.27.0\nanyhow v1.0.0 (*)\nsmop-macros v0.4.0\n\n";
        assert_eq!(count_crates(tree), 3);
    }

    #[test]
    fn the_modules_are_the_source_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        for module in MODULES {
            assert!(root.join(format!("src/{module}.rs")).is_file(), "{module}");
        }
    }
}
