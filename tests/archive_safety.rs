//! Containment tests for `archive::extract_*` and `archive::extract`.
//!
//! Each test builds an archive with raw entry names, extracts it, and checks
//! that nothing outside `dest` changes. Each containment test runs once with
//! the `extract_*` function of the format and once with `extract`. The
//! proptest at the end does the same for generated entries.

#![cfg(feature = "archive")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use smop::archive;
use tempfile::TempDir;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Zip,
    Tar,
    TarGz,
}

/// An archive kind and the function that extracts it.
#[derive(Clone, Copy, Debug)]
struct Format {
    kind: Kind,
    /// `true`: `archive::extract`. `false`: the `extract_*` of the kind.
    by_extension: bool,
}

impl Format {
    const ZIP: Self = Self::named(Kind::Zip);
    const TAR: Self = Self::named(Kind::Tar);
    const TAR_GZ: Self = Self::named(Kind::TarGz);

    const fn named(kind: Kind) -> Self {
        Self {
            kind,
            by_extension: false,
        }
    }

    const fn by_extension(self) -> Self {
        Self {
            kind: self.kind,
            by_extension: true,
        }
    }
}

const FORMATS: [Format; 6] = [
    Format::ZIP,
    Format::TAR,
    Format::TAR_GZ,
    Format::ZIP.by_extension(),
    Format::TAR.by_extension(),
    Format::TAR_GZ.by_extension(),
];
const TAR_FORMATS: [Format; 4] = [
    Format::TAR,
    Format::TAR_GZ,
    Format::TAR.by_extension(),
    Format::TAR_GZ.by_extension(),
];

#[derive(Clone, Debug)]
enum Entry {
    File(String, String),
    Dir(String),
    // Only the Unix tests make links.
    #[cfg_attr(not(unix), allow(dead_code))]
    Link(String, String),
    Hard(String, String),
}

fn file(name: &str, data: &str) -> Entry {
    Entry::File(name.into(), data.into())
}

#[cfg(unix)]
fn link(name: &str, target: &str) -> Entry {
    Entry::Link(name.into(), target.into())
}

/// Writes `entries` as a tar stream. Names go into the header as raw bytes, so
/// `..` and `/` stay as they are.
fn tar_bytes(entries: &[Entry]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for entry in entries {
        let mut header = tar::Header::new_gnu();
        let (name, data, kind, target) = match entry {
            Entry::File(n, d) => (n, d.as_str(), tar::EntryType::Regular, None),
            Entry::Dir(n) => (n, "", tar::EntryType::Directory, None),
            Entry::Link(n, t) => (n, "", tar::EntryType::Symlink, Some(t)),
            Entry::Hard(n, t) => (n, "", tar::EntryType::Link, Some(t)),
        };
        let raw = &mut header.as_gnu_mut().unwrap().name;
        raw.fill(0);
        raw[..name.len()].copy_from_slice(name.as_bytes());
        header.set_entry_type(kind);
        header.set_size(data.len() as u64);
        header.set_mode(if kind == tar::EntryType::Directory {
            0o755
        } else {
            0o644
        });
        if let Some(target) = target {
            header.set_link_name_literal(target).unwrap();
        }
        header.set_cksum();
        builder.append(&header, data.as_bytes()).unwrap();
    }
    builder.into_inner().unwrap()
}

/// Writes `entries` as a zip file. Zip has no hard links, and the zip writer
/// refuses a name it already has, so the test skips those entries.
fn zip_bytes(entries: &[Entry]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    let mut seen = std::collections::HashSet::new();
    for entry in entries {
        let (Entry::File(n, _) | Entry::Dir(n) | Entry::Link(n, _) | Entry::Hard(n, _)) = entry;
        let key = if matches!(entry, Entry::Dir(_)) {
            format!("{}/", n.trim_end_matches('/'))
        } else {
            n.clone()
        };
        if !seen.insert(key) {
            continue;
        }
        match entry {
            Entry::File(n, d) => {
                zip.start_file(n.as_str(), options).unwrap();
                zip.write_all(d.as_bytes()).unwrap();
            }
            Entry::Dir(n) => zip.add_directory(n.as_str(), options).unwrap(),
            Entry::Link(n, t) => zip.add_symlink(n.as_str(), t.as_str(), options).unwrap(),
            Entry::Hard(..) => {}
        }
    }
    zip.finish().unwrap().into_inner()
}

/// Returns `entries` as archive bytes of `kind`.
fn archive_bytes(kind: Kind, entries: &[Entry]) -> Vec<u8> {
    match kind {
        Kind::Zip => zip_bytes(entries),
        Kind::Tar => tar_bytes(entries),
        Kind::TarGz => {
            let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            gz.write_all(&tar_bytes(entries)).unwrap();
            gz.finish().unwrap()
        }
    }
}

/// Writes the archive into `dir` and returns its path.
fn write_archive(format: Format, entries: &[Entry], dir: &Path) -> PathBuf {
    let name = match format.kind {
        Kind::Zip => "archive.zip",
        Kind::Tar => "archive.tar",
        Kind::TarGz => "archive.tar.gz",
    };
    let path = dir.join(name);
    fs::write(&path, archive_bytes(format.kind, entries)).unwrap();
    path
}

fn extract(format: Format, archive: &Path, dest: &Path) -> anyhow::Result<()> {
    if format.by_extension {
        return archive::extract(archive, dest);
    }
    match format.kind {
        Kind::Zip => archive::extract_zip(archive, dest),
        Kind::Tar => archive::extract_tar(archive, dest),
        Kind::TarGz => archive::extract_tar_gz(archive, dest),
    }
}

/// A temp tree: `root/outside/secret.txt`, and `dest` at `root/w1/w2/dest`,
/// so three `..` steps reach `root`.
struct Fixture {
    root: TempDir,
    dest: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        fs::create_dir_all(root.path().join("outside/dir")).unwrap();
        fs::write(root.path().join("outside/secret.txt"), "secret").unwrap();
        fs::create_dir(root.path().join("arch")).unwrap();
        let dest = root.path().join("w1/w2/dest");
        fs::create_dir_all(&dest).unwrap();
        Self { root, dest }
    }

    fn outside(&self) -> PathBuf {
        self.root.path().join("outside")
    }

    /// Where tests write archives.
    fn arch(&self) -> PathBuf {
        self.root.path().join("arch")
    }

    /// Everything under `root` except `dest` and the archives.
    fn snapshot(&self) -> BTreeMap<PathBuf, String> {
        let mut out = BTreeMap::new();
        walk(self.root.path(), &mut |path, meta| {
            if path.starts_with(&self.dest) || path.starts_with(self.arch()) {
                return false;
            }
            let value = if meta.is_symlink() {
                format!("link {}", fs::read_link(path).unwrap().display())
            } else if meta.is_file() {
                format!("file {}", fs::read_to_string(path).unwrap_or_default())
            } else {
                "dir".into()
            };
            out.insert(path.to_path_buf(), value);
            true
        });
        out
    }

    /// Extracts `entries` with `format`. Asserts that nothing outside `dest`
    /// changed, and returns the result.
    fn run(&self, format: Format, entries: &[Entry]) -> anyhow::Result<()> {
        let before = self.snapshot();
        let archive = write_archive(format, entries, &self.arch());
        let result = extract(format, &archive, &self.dest);
        fs::remove_file(&archive).unwrap();
        assert_eq!(
            before,
            self.snapshot(),
            "{format:?} changed files outside dest"
        );
        result
    }
}

/// Visits every path below `dir` without following symbolic links. `visit`
/// returns `false` to skip a directory.
fn walk(dir: &Path, visit: &mut dyn FnMut(&Path, &fs::Metadata) -> bool) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let meta = fs::symlink_metadata(&path).unwrap();
        if visit(&path, &meta) && meta.is_dir() {
            walk(&path, visit);
        }
    }
}

/// Asserts that `result` is an error whose top-level message names `entry`.
fn assert_names(result: anyhow::Result<()>, entry: &str, format: Format) {
    let err = result.expect_err(&format!("{format:?}: expected an error for {entry:?}"));
    let msg = err.to_string();
    assert!(msg.contains(&format!("\"{entry}\"")), "{format:?}: {msg}");
}

#[test]
fn escaping_entry_errors_and_names_it() {
    for format in FORMATS {
        let fx = Fixture::new();
        let result = fx.run(
            format,
            &[file("good.txt", "ok"), file("../escape.txt", "x")],
        );
        let msg = result.as_ref().map_err(ToString::to_string).unwrap_err();
        assert!(
            msg.contains("archive."),
            "{format:?}: names the archive: {msg}"
        );
        assert_names(result, "../escape.txt", format);
    }
}

#[test]
fn nested_escaping_entry_errors() {
    for format in FORMATS {
        let fx = Fixture::new();
        let result = fx.run(format, &[file("sub/../../evil.txt", "x")]);
        assert_names(result, "sub/../../evil.txt", format);
    }
}

#[test]
fn deep_escape_does_not_reach_a_sibling_directory() {
    for format in FORMATS {
        let fx = Fixture::new();
        let result = fx.run(format, &[file("../../../outside/secret.txt", "owned")]);
        assert_names(result, "../../../outside/secret.txt", format);
    }
}

#[test]
fn absolute_entry_errors() {
    for format in FORMATS {
        let fx = Fixture::new();
        let result = fx.run(format, &[file("/abs_evil.txt", "x")]);
        assert_names(result, "/abs_evil.txt", format);
        assert!(!fx.dest.join("abs_evil.txt").exists(), "{format:?}");
    }
}

#[test]
fn absolute_directory_entry_errors() {
    for format in FORMATS {
        let fx = Fixture::new();
        let result = fx.run(format, &[Entry::Dir("/abs_dir/".into())]);
        let err = result.expect_err(&format!("{format:?}"));
        assert!(err.to_string().contains("/abs_dir"), "{format:?}: {err}");
        assert!(!fx.dest.join("abs_dir").exists(), "{format:?}");
    }
}

#[test]
fn dotdot_inside_dest_extracts_the_same_in_all_formats() {
    for format in FORMATS {
        let fx = Fixture::new();
        fx.run(format, &[file("a/../inside.txt", "in")]).unwrap();
        assert_eq!(
            fs::read_to_string(fx.dest.join("inside.txt")).unwrap(),
            "in",
            "{format:?}"
        );
    }
}

#[test]
fn dot_and_empty_directory_entries_extract() {
    for format in FORMATS {
        let fx = Fixture::new();
        let entries = [
            Entry::Dir("./".into()),
            Entry::Dir("d/".into()),
            file("./d/f.txt", "f"),
        ];
        fx.run(format, &entries).unwrap();
        assert_eq!(
            fs::read_to_string(fx.dest.join("d/f.txt")).unwrap(),
            "f",
            "{format:?}"
        );
    }
}

#[test]
fn missing_dest_is_created() {
    for format in FORMATS {
        let fx = Fixture::new();
        let archive = write_archive(format, &[file("f.txt", "f")], &fx.arch());
        let dest = fx.root.path().join("new/dest");
        extract(format, &archive, &dest).unwrap();
        assert_eq!(
            fs::read_to_string(dest.join("f.txt")).unwrap(),
            "f",
            "{format:?}"
        );
    }
}

#[test]
fn issue_example_errors() {
    let entries = [
        file("good.txt", "ok"),
        file("../evil.txt", "x"),
        file("sub/../../evil2.txt", "x"),
        file("/abs_evil.txt", "x"),
        file("a/../inside.txt", "in"),
    ];
    for format in FORMATS {
        let fx = Fixture::new();
        assert_names(fx.run(format, &entries), "../evil.txt", format);
    }
}

#[test]
fn tar_hard_link_outside_dest_errors() {
    for format in TAR_FORMATS {
        let fx = Fixture::new();
        let entries = [Entry::Hard(
            "h.txt".into(),
            "../../../outside/secret.txt".into(),
        )];
        assert_names(fx.run(format, &entries), "h.txt", format);
        assert!(
            fs::symlink_metadata(fx.dest.join("h.txt")).is_err(),
            "{format:?}"
        );
    }
}

#[test]
fn tar_absolute_hard_link_errors() {
    for format in TAR_FORMATS {
        let fx = Fixture::new();
        let target = fx.outside().join("secret.txt");
        let entries = [Entry::Hard(
            "h.txt".into(),
            target.to_string_lossy().into_owned(),
        )];
        assert_names(fx.run(format, &entries), "h.txt", format);
    }
}

#[test]
fn tar_hard_link_inside_dest_extracts() {
    for format in TAR_FORMATS {
        let fx = Fixture::new();
        let entries = [
            file("a.txt", "x"),
            Entry::Hard("sub/../b.txt".into(), "a.txt".into()),
        ];
        fx.run(format, &entries).unwrap();
        assert_eq!(
            fs::read_to_string(fx.dest.join("b.txt")).unwrap(),
            "x",
            "{format:?}"
        );
    }
}

#[test]
fn tar_hard_link_to_itself_keeps_the_file() {
    // GNU tar writes this for `tar cf x.tar a a`.
    for format in TAR_FORMATS {
        let fx = Fixture::new();
        let entries = [
            file("a.txt", "x"),
            Entry::Hard("a.txt".into(), "a.txt".into()),
        ];
        fx.run(format, &entries).unwrap();
        assert_eq!(
            fs::read_to_string(fx.dest.join("a.txt")).unwrap(),
            "x",
            "{format:?}"
        );
    }
}

#[test]
fn tar_hard_link_to_missing_file_errors() {
    for format in TAR_FORMATS {
        let fx = Fixture::new();
        let entries = [Entry::Hard("b.txt".into(), "missing.txt".into())];
        assert_names(fx.run(format, &entries), "b.txt", format);
    }
}

#[test]
fn empty_file_name_errors() {
    for format in TAR_FORMATS {
        let fx = Fixture::new();
        assert_names(fx.run(format, &[file(".", "x")]), ".", format);
    }
}

// ---- `extract` selects the format from the extension ----

/// Each extension that `extract` knows, and its kind.
const EXTENSIONS: [(&str, Kind); 4] = [
    (".zip", Kind::Zip),
    (".tar", Kind::Tar),
    (".tar.gz", Kind::TarGz),
    (".tgz", Kind::TarGz),
];

/// Returns each mix of upper and lower case letters of `s`.
fn letter_cases(s: &str) -> Vec<String> {
    let letters = s.chars().filter(char::is_ascii_alphabetic).count();
    (0..1_u32 << letters)
        .map(|mask| {
            let mut bit = 0;
            s.chars()
                .map(|c| {
                    if !c.is_ascii_alphabetic() {
                        return c;
                    }
                    bit += 1;
                    if mask & (1 << (bit - 1)) == 0 {
                        c
                    } else {
                        c.to_ascii_uppercase()
                    }
                })
                .collect()
        })
        .collect()
}

#[test]
fn letter_cases_gives_each_mix() {
    assert_eq!(letter_cases(".tgz").len(), 8);
    assert_eq!(letter_cases(".tar.gz").len(), 32);
    assert!(letter_cases(".tar.gz").contains(&".TaR.gZ".to_string()));
    let all: std::collections::BTreeSet<String> = letter_cases(".tar.gz").into_iter().collect();
    assert_eq!(all.len(), 32, "each mix is different");
}

#[test]
fn extract_dispatches_each_extension_in_any_letter_case() {
    for (ext, kind) in EXTENSIONS {
        for ext in letter_cases(ext) {
            let fx = Fixture::new();
            let archive = fx.arch().join(format!("a{ext}"));
            fs::write(&archive, archive_bytes(kind, &[file("f.txt", &ext)])).unwrap();
            archive::extract(&archive, &fx.dest).unwrap_or_else(|e| panic!("{ext}: {e:#}"));
            assert_eq!(fs::read_to_string(fx.dest.join("f.txt")).unwrap(), ext);
        }
    }
}

/// The extension selects the extractor, not the content. Each extractor gives
/// its own error for a file of a different kind.
#[test]
fn extract_uses_the_extension_not_the_content() {
    let cases = [
        ("a.zip", Kind::Tar, "ZIP archive"),
        ("a.tar", Kind::Zip, "TAR archive"),
        ("a.tar.gz", Kind::Tar, "gzip"),
        ("a.tgz", Kind::Zip, "gzip"),
    ];
    for (name, kind, expected) in cases {
        let fx = Fixture::new();
        let archive = fx.arch().join(name);
        fs::write(&archive, archive_bytes(kind, &[file("f.txt", "x")])).unwrap();
        let err = archive::extract(&archive, &fx.dest).expect_err(name);
        let msg = format!("{err:#}");
        assert!(msg.contains(expected), "{name}: {msg}");
        assert!(!fx.dest.join("f.txt").exists(), "{name}");
    }
}

#[test]
fn extract_unknown_extension_errors_and_lists_the_supported_ones() {
    let names = [
        "a.rar",
        "a.tar.bz2",
        "a.tar.xz",
        "a.gz",
        "a.tbz",
        "a.zip.bak",
        "a.tar.gz.part",
        "a",
        "azip",
        "atar",
        "a.zip ",
        "a.zip.",
        "a.tar.gz.",
        ".gz",
    ];
    for name in names {
        let fx = Fixture::new();
        let archive = fx.arch().join(name);
        let dest = fx.root.path().join("new");
        let err = archive::extract(&archive, &dest).expect_err(name);
        let msg = err.to_string();
        assert!(
            msg.contains(".zip, .tar, .tar.gz, .tgz"),
            "{name}: lists the supported extensions: {msg}"
        );
        assert!(msg.contains("any letter case"), "{name}: {msg}");
        assert!(
            msg.contains(&archive.display().to_string()),
            "{name}: names the file: {msg}"
        );
        assert!(!dest.exists(), "{name}: makes no directory");
    }
}

/// Only the file name counts. A directory name with an extension does not.
#[test]
fn extract_reads_the_extension_of_the_file_name_only() {
    let fx = Fixture::new();
    let dir = fx.arch().join("x.zip");
    fs::create_dir(&dir).unwrap();
    let archive = dir.join("a.tar");
    fs::write(&archive, archive_bytes(Kind::Tar, &[file("f.txt", "t")])).unwrap();
    archive::extract(&archive, &fx.dest).unwrap();
    assert_eq!(fs::read_to_string(fx.dest.join("f.txt")).unwrap(), "t");

    let err = archive::extract(fx.arch().join("a.tar.gz").join("b"), &fx.dest).unwrap_err();
    assert!(
        err.to_string().contains("Unknown archive extension"),
        "{err}"
    );
}

/// `extract` uses the extension of a file name that is not UTF-8. APFS
/// refuses such a name, so the file does not exist. The open error shows the
/// selected function.
#[cfg(unix)]
#[test]
fn extract_non_utf8_file_name_selects_by_extension() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let fx = Fixture::new();
    let archive = fx.arch().join(OsStr::from_bytes(b"\xff\xfe.TGZ"));
    let msg = archive::extract(&archive, &fx.dest)
        .unwrap_err()
        .to_string();
    assert!(msg.contains("Failed to open TAR.GZ archive"), "{msg}");
}

/// A missing file with a known extension gives the error of its extractor.
#[test]
fn extract_missing_file_names_it() {
    let fx = Fixture::new();
    let archive = fx.arch().join("missing.tgz");
    let err = archive::extract(&archive, &fx.dest).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("TAR.GZ"), "{msg}");
    assert!(msg.contains(&archive.display().to_string()), "{msg}");
}

#[cfg(windows)]
#[test]
fn windows_stream_name_errors() {
    for format in FORMATS {
        let fx = Fixture::new();
        assert_names(
            fx.run(format, &[file("a.txt:evil", "x")]),
            "a.txt:evil",
            format,
        );
    }
}

#[cfg(unix)]
mod links {
    use std::path::Component;

    use proptest::prelude::*;

    use super::*;

    fn is_link(path: &Path) -> bool {
        fs::symlink_metadata(path).is_ok_and(|m| m.is_symlink())
    }

    #[test]
    fn escaping_link_errors_and_is_not_left_behind() {
        for format in FORMATS {
            let fx = Fixture::new();
            let result = fx.run(format, &[link("link", ".."), file("link/evil.txt", "x")]);
            assert_names(result, "link", format);
            assert!(
                fs::symlink_metadata(fx.dest.join("link")).is_err(),
                "{format:?}"
            );
        }
    }

    #[test]
    fn absolute_link_errors() {
        for format in FORMATS {
            let fx = Fixture::new();
            let target = fx.outside();
            let result = fx.run(format, &[link("abs", target.to_str().unwrap())]);
            assert_names(result, "abs", format);
            assert!(!is_link(&fx.dest.join("abs")), "{format:?}");
        }
    }

    #[test]
    fn links_inside_dest_extract() {
        for format in FORMATS {
            let fx = Fixture::new();
            let entries = [
                file("real/f.txt", "f"),
                link("to_dir", "real"),
                link("sub/up", "../real/f.txt"),
                link("dangling", "not/yet/here"),
                file("to_dir/g.txt", "g"),
            ];
            fx.run(format, &entries).unwrap();
            assert!(is_link(&fx.dest.join("to_dir")), "{format:?}");
            assert_eq!(fs::read_to_string(fx.dest.join("sub/up")).unwrap(), "f");
            assert!(is_link(&fx.dest.join("dangling")), "{format:?}");
            // A write through a link that stays inside `dest` is allowed.
            assert_eq!(fs::read_to_string(fx.dest.join("real/g.txt")).unwrap(), "g");
        }
    }

    #[test]
    fn link_chain_that_escapes_errors() {
        for format in FORMATS {
            let fx = Fixture::new();
            let result = fx.run(format, &[link("a", "."), link("b", "a/..")]);
            assert_names(result, "b", format);
            assert!(!is_link(&fx.dest.join("b")), "{format:?}");
        }
    }

    #[test]
    fn later_link_that_redirects_an_earlier_link_errors() {
        for format in FORMATS {
            let fx = Fixture::new();
            let result = fx.run(
                format,
                &[link("b", "p/../x"), link("p", "q"), file("q/f", "")],
            );
            assert!(result.is_ok(), "{format:?}: b stays inside: {result:?}");
            let fx = Fixture::new();
            let result = fx.run(format, &[link("b", "p/../x"), link("p", ".")]);
            assert_names(result, "b", format);
            assert!(!is_link(&fx.dest.join("b")), "{format:?}");
        }
    }

    #[test]
    fn link_target_must_stay_inside_by_name_too() {
        for format in FORMATS {
            let fx = Fixture::new();
            let entries = [
                Entry::Dir("a/b/".into()),
                link("l", "a/b"),
                link("x", "l/../../y"),
            ];
            assert_names(fx.run(format, &entries), "x", format);
            assert!(!is_link(&fx.dest.join("x")), "{format:?}");
        }
    }

    #[test]
    fn tar_link_with_a_trailing_slash_is_a_link() {
        for format in TAR_FORMATS {
            let fx = Fixture::new();
            fx.run(format, &[Entry::Dir("d/".into()), link("l/", "d")])
                .unwrap();
            assert!(is_link(&fx.dest.join("l")), "{format:?}");
        }
    }

    #[test]
    fn tar_link_keeps_its_time() {
        let fx = Fixture::new();
        let entries = [file("t", "t"), link("l", "t")];
        let archive = fx.arch().join("a.tar");
        let mut bytes = tar_bytes(&entries);
        // The link header follows the file header and one data block.
        let mut header = tar::Header::from_byte_slice(&bytes[1024..1536]).clone();
        header.set_mtime(1_000_000_000);
        header.set_cksum();
        bytes[1024..1536].copy_from_slice(header.as_bytes());
        fs::write(&archive, bytes).unwrap();
        archive::extract_tar(&archive, &fx.dest).unwrap();
        let meta = fs::symlink_metadata(fx.dest.join("l")).unwrap();
        let mtime = meta
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        assert_eq!(mtime.as_secs(), 1_000_000_000);
    }

    #[test]
    fn tar_hard_link_to_a_link_is_a_link() {
        for format in TAR_FORMATS {
            let fx = Fixture::new();
            let entries = [
                file("t", "t"),
                link("sl", "t"),
                Entry::Hard("hl".into(), "sl".into()),
            ];
            fx.run(format, &entries).unwrap();
            assert!(is_link(&fx.dest.join("hl")), "{format:?}");
        }
    }

    #[test]
    fn tar_hard_link_to_a_link_is_checked_at_its_new_place() {
        for format in TAR_FORMATS {
            let fx = Fixture::new();
            // `sub/sl -> ../t` stays inside. The same link at `hl` goes out.
            let entries = [
                file("t", "t"),
                link("sub/sl", "../t"),
                Entry::Hard("hl".into(), "sub/sl".into()),
            ];
            assert_names(fx.run(format, &entries), "hl", format);
            assert!(!is_link(&fx.dest.join("hl")), "{format:?}");
        }
    }

    #[test]
    fn error_keeps_both_the_entry_error_and_the_link_error() {
        for format in FORMATS {
            let fx = Fixture::new();
            let entries = [link("b", "p/../x"), link("p", "."), file("../evil", "")];
            let err = fx.run(format, &entries).unwrap_err();
            let all = format!("{err:#}");
            assert!(err.to_string().contains("\"../evil\""), "{format:?}: {all}");
            assert!(all.contains("\"b\""), "{format:?}: {all}");
            assert!(!is_link(&fx.dest.join("b")), "{format:?}");
        }
    }

    #[test]
    fn link_loop_errors() {
        for format in FORMATS {
            let fx = Fixture::new();
            let result = fx.run(format, &[link("a", "b"), link("b", "a")]);
            assert!(result.is_err(), "{format:?}");
            // One link is removed. The other then only dangles inside `dest`.
            assert!(!(is_link(&fx.dest.join("a")) && is_link(&fx.dest.join("b"))));
        }
    }

    #[test]
    fn existing_link_in_dest_that_escapes_is_not_followed() {
        for format in FORMATS {
            let fx = Fixture::new();
            std::os::unix::fs::symlink(fx.outside(), fx.dest.join("out")).unwrap();
            let result = fx.run(format, &[file("out/x.txt", "x")]);
            assert_names(result, "out/x.txt", format);
        }
    }

    #[test]
    fn file_replaces_an_existing_link_instead_of_writing_through_it() {
        for format in FORMATS {
            let fx = Fixture::new();
            let path = fx.dest.join("f");
            std::os::unix::fs::symlink(fx.outside().join("secret.txt"), &path).unwrap();
            fx.run(format, &[file("f", "new")]).unwrap();
            assert!(!is_link(&path), "{format:?}");
            assert_eq!(fs::read_to_string(&path).unwrap(), "new", "{format:?}");
        }
    }

    #[test]
    fn link_replaces_an_existing_file() {
        for format in FORMATS {
            let fx = Fixture::new();
            fs::write(fx.dest.join("l"), "old").unwrap();
            fx.run(format, &[file("t", "t"), link("l", "t")]).unwrap();
            assert!(is_link(&fx.dest.join("l")), "{format:?}");
        }
    }

    #[test]
    fn tar_keeps_the_executable_bit() {
        use std::os::unix::fs::PermissionsExt;
        let fx = Fixture::new();
        let src = fx.root.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("run.sh"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(src.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        let tar = fx.root.path().join("a.tar");
        archive::create_tar(&src, &tar).unwrap();
        archive::extract_tar(&tar, &fx.dest).unwrap();
        let mode = fs::metadata(fx.dest.join("run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "mode {mode:o}");
    }

    #[test]
    fn tar_read_only_directory_entry_does_not_block_its_files() {
        use std::os::unix::fs::PermissionsExt;
        let fx = Fixture::new();
        let entries = [Entry::Dir("ro/".into()), file("ro/f.txt", "f")];
        let archive = write_archive(Format::TAR, &entries, &fx.arch());
        // Make the directory entry read-only.
        let mut bytes = fs::read(&archive).unwrap();
        let mut header = tar::Header::from_byte_slice(&bytes[..512]).clone();
        header.set_mode(0o555);
        header.set_cksum();
        bytes[..512].copy_from_slice(header.as_bytes());
        fs::write(&archive, bytes).unwrap();
        archive::extract_tar(&archive, &fx.dest).unwrap();
        assert_eq!(fs::read_to_string(fx.dest.join("ro/f.txt")).unwrap(), "f");
        // Let TempDir remove the tree.
        fs::set_permissions(fx.dest.join("ro"), fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Sets the mode of the header at `index`. The entries before it must
    /// have no data.
    fn tar_with_mode(entries: &[Entry], index: usize, mode: u32) -> Vec<u8> {
        let mut bytes = tar_bytes(entries);
        let at = index * 512;
        let mut header = tar::Header::from_byte_slice(&bytes[at..at + 512]).clone();
        header.set_mode(mode);
        header.set_cksum();
        bytes[at..at + 512].copy_from_slice(header.as_bytes());
        bytes
    }

    /// A GNU header for a file with a name that ends in `/` makes a
    /// directory (old BSD tar). Its mode must not stop the link check. Run
    /// as root, this test passes also without the fix.
    #[test]
    fn tar_bsd_directory_entry_does_not_hide_an_escaping_link() {
        use std::os::unix::fs::PermissionsExt;
        for format in [Format::TAR, Format::TAR.by_extension()] {
            for mode in [0o555, 0o444] {
                let fx = Fixture::new();
                let entries = [link("ro/l", "p/.."), link("ro/p", ".."), file("ro/", "")];
                let archive = fx.arch().join("a.tar");
                fs::write(&archive, tar_with_mode(&entries, 2, mode)).unwrap();
                let result = extract(format, &archive, &fx.dest);
                let ro = fx.dest.join("ro");
                fs::set_permissions(&ro, fs::Permissions::from_mode(0o755)).unwrap();
                assert_names(result, "ro/l", format);
                assert!(!is_link(&ro.join("l")), "{format:?}: mode {mode:o}");
            }
        }
    }

    // ---- Containment property ----

    /// Resolves `path` like the kernel would, as far as it exists, then
    /// appends the rest lexically. It uses `fs::canonicalize`, not the code
    /// under test.
    fn oracle_resolve(path: &Path) -> PathBuf {
        let mut existing = path;
        let mut rest = Vec::new();
        let base = loop {
            if let Ok(canon) = fs::canonicalize(existing) {
                break canon;
            }
            rest.push(existing.components().next_back().unwrap());
            existing = existing.parent().unwrap();
        };
        let mut out = base;
        for part in rest.into_iter().rev() {
            match part {
                Component::ParentDir => {
                    out.pop();
                }
                Component::Normal(p) => out.push(p),
                _ => {}
            }
        }
        out
    }

    fn component() -> impl Strategy<Value = &'static str> {
        prop::sample::select(vec!["a", "b", "..", ".", "outside", "secret.txt"])
    }

    /// Relative names only: an absolute name would write to the real root
    /// directory if the code under test had a bug. Unit tests cover them.
    fn name() -> impl Strategy<Value = String> {
        prop::collection::vec(component(), 1..5).prop_map(|parts| parts.join("/"))
    }

    fn entry() -> impl Strategy<Value = Entry> {
        prop_oneof![
            (name(), "[a-z]{0,3}").prop_map(|(n, d)| Entry::File(n, d)),
            name().prop_map(Entry::Dir),
            (name(), name()).prop_map(|(n, t)| Entry::Link(n, t)),
            (name(), name()).prop_map(|(n, t)| Entry::Hard(n, t)),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        /// Every path under `dest` resolves under `dest`, no file under
        /// `dest` shares an inode with a file outside it, and nothing outside
        /// `dest` changes (`Fixture::run` checks that).
        #[test]
        fn extraction_stays_inside_dest(entries in prop::collection::vec(entry(), 1..7)) {
            use std::os::unix::fs::MetadataExt;
            for format in FORMATS {
                let fx = Fixture::new();
                let mut outside_inodes = Vec::new();
                walk(fx.root.path(), &mut |path, meta| {
                    if path.starts_with(&fx.dest) {
                        return false;
                    }
                    outside_inodes.push((meta.dev(), meta.ino()));
                    true
                });
                let _ = fx.run(format, &entries);
                let canon_dest = fs::canonicalize(&fx.dest).unwrap();
                walk(&fx.dest, &mut |path, meta| {
                    let real = if meta.is_symlink() {
                        let parent = fs::canonicalize(path.parent().unwrap()).unwrap();
                        oracle_resolve(&parent.join(fs::read_link(path).unwrap()))
                    } else {
                        fs::canonicalize(path).unwrap()
                    };
                    assert!(
                        real.starts_with(&canon_dest),
                        "{format:?}: {} resolves to {}",
                        path.display(),
                        real.display()
                    );
                    if meta.is_file() {
                        assert!(
                            !outside_inodes.contains(&(meta.dev(), meta.ino())),
                            "{format:?}: {} is a hard link to a file outside dest",
                            path.display()
                        );
                    }
                    true
                });
            }
        }
    }
}
