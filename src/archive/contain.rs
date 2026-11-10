//! Keeps archive extraction inside the destination directory (ADR 0008).
//!
//! The rules, for all formats:
//! - An absolute entry path is an error.
//! - A `..` step removes the step before it. If no step is left to remove,
//!   the entry is an error.
//! - A write does not follow a link out of the destination. Links that are
//!   on disk are followed when a path is checked.
//! - A link target must stay inside the destination. An absolute target is
//!   an error.

use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow};

/// The maximum number of links in one path, as `MAXSYMLINKS` on Linux.
const MAX_LINK_HOPS: u32 = 40;

/// Why an archive entry is unsafe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Unsafe {
    Absolute,
    Outside,
    Empty,
    AbsoluteLink,
    LinkOutside,
    LinkLoop,
    BadName,
}

impl Unsafe {
    const fn reason(self) -> &'static str {
        match self {
            Self::Absolute => "the path is absolute",
            Self::Outside => "the path goes outside the destination",
            Self::Empty => "the path is empty",
            Self::AbsoluteLink => "the link target is absolute",
            Self::LinkOutside => "the link target is outside the destination",
            Self::LinkLoop => "the path goes through too many links",
            Self::BadName => "the name is not safe on this system",
        }
    }
}

/// What a path is: an entry path or a link target. This sets the errors.
#[derive(Debug, Clone, Copy)]
enum Kind {
    Entry,
    Link,
}

impl Kind {
    const fn absolute(self) -> Unsafe {
        match self {
            Self::Entry => Unsafe::Absolute,
            Self::Link => Unsafe::AbsoluteLink,
        }
    }

    const fn outside(self) -> Unsafe {
        match self {
            Self::Entry => Unsafe::Outside,
            Self::Link => Unsafe::LinkOutside,
        }
    }
}

/// One step of a relative path.
enum Step {
    Up,
    Name(OsString),
}

/// Splits `path` into steps. `.` steps are removed.
fn steps(path: &Path, absolute: Unsafe) -> Result<Vec<Step>, Unsafe> {
    path.components()
        .filter_map(|part| match part {
            Component::CurDir => None,
            Component::Prefix(_) | Component::RootDir => Some(Err(absolute)),
            Component::ParentDir => Some(Ok(Step::Up)),
            Component::Normal(name) => Some(Ok(Step::Name(name.to_owned()))),
        })
        .collect()
}

/// Rejects a name that Windows can use to write to a stream or a device.
fn check_name(name: &OsStr) -> Result<(), Unsafe> {
    if cfg!(windows) && name.to_string_lossy().contains(':') {
        return Err(Unsafe::BadName);
    }
    Ok(())
}

/// Normalizes an entry path by name only. It does not read the disk.
///
/// The result has only normal names. An empty result is the destination.
pub(super) fn normalize(path: &Path) -> Result<Vec<OsString>, Unsafe> {
    let mut out = Vec::new();
    for step in steps(path, Unsafe::Absolute)? {
        match step {
            Step::Up => {
                out.pop().ok_or(Unsafe::Outside)?;
            }
            Step::Name(name) => {
                check_name(&name)?;
                out.push(name);
            }
        }
    }
    Ok(out)
}

fn join(root: &Path, parts: &[OsString]) -> PathBuf {
    let mut path = root.to_path_buf();
    path.extend(parts);
    path
}

/// Resolves `rel` from the directory `base` below `root`, and follows each
/// link that is on disk. The result has no links, and it is below `root`.
fn resolve(
    root: &Path,
    base: &[OsString],
    rel: &Path,
    kind: Kind,
) -> Result<Vec<OsString>, Unsafe> {
    let mut out = base.to_vec();
    let mut todo = VecDeque::from(steps(rel, kind.absolute())?);
    let mut hops = 0;
    while let Some(step) = todo.pop_front() {
        match step {
            Step::Up => {
                out.pop().ok_or_else(|| kind.outside())?;
            }
            Step::Name(name) => {
                out.push(name);
                let path = join(root, &out);
                if fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_symlink()) {
                    hops += 1;
                    if hops > MAX_LINK_HOPS {
                        return Err(Unsafe::LinkLoop);
                    }
                    let target = fs::read_link(&path).map_err(|_| kind.outside())?;
                    out.pop();
                    // An absolute link on disk goes outside `root`.
                    for step in steps(&target, kind.outside())?.into_iter().rev() {
                        todo.push_front(step);
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Removes a file or a link at `path`, so that a write does not go through
/// it. It does not remove a directory.
fn remove_existing(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.is_dir() => fs::remove_file(path),
        _ => Ok(()),
    }
}

#[cfg(unix)]
fn make_symlink(target: &Path, path: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, path)
}

#[cfg(windows)]
fn make_symlink(target: &Path, path: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(target, path)
}

#[cfg(not(any(unix, windows)))]
fn make_symlink(_target: &Path, _path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "symbolic links are not supported on this platform",
    ))
}

/// Writes archive entries below one destination directory.
pub(super) struct Extractor<'a> {
    archive: &'a Path,
    /// The canonical destination.
    root: PathBuf,
    /// The links that this extraction made, and their entry names.
    links: Vec<(Vec<OsString>, PathBuf)>,
}

impl<'a> Extractor<'a> {
    /// Makes `dest` if necessary, then runs `body`. Then it checks the links
    /// again, also after an error, so that no link that goes out of `dest`
    /// stays on disk.
    pub(super) fn run(
        archive: &'a Path,
        dest: &Path,
        body: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        fs::create_dir_all(dest)
            .with_context(|| format!("Failed to create directory: {}", dest.display()))?;
        let root = fs::canonicalize(dest)
            .with_context(|| format!("Failed to resolve directory: {}", dest.display()))?;
        let mut this = Self {
            archive,
            root,
            links: Vec::new(),
        };
        let result = body(&mut this);
        let links = this.check_links();
        result.and(links)
    }

    fn unsafe_entry(&self, name: &Path, why: Unsafe) -> anyhow::Error {
        anyhow!(
            "Unsafe entry \"{}\" in archive {}: {}",
            name.display(),
            self.archive.display(),
            why.reason()
        )
    }

    fn check<T>(&self, name: &Path, result: Result<T, Unsafe>) -> Result<T> {
        result.map_err(|why| self.unsafe_entry(name, why))
    }

    /// Returns the path for a directory entry. It returns `None` for the
    /// destination itself.
    pub(super) fn dir(&self, name: &Path) -> Result<Option<PathBuf>> {
        let lexical: PathBuf = self.check(name, normalize(name))?.iter().collect();
        let real = self.check(name, resolve(&self.root, &[], &lexical, Kind::Entry))?;
        Ok((!real.is_empty()).then(|| join(&self.root, &real)))
    }

    /// Finds the path for a file or link entry and makes its parent
    /// directory. The last name is not followed if it is a link.
    fn place(&self, name: &Path) -> Result<(Vec<OsString>, PathBuf)> {
        let mut lexical = self.check(name, normalize(name))?;
        let last = self.check(name, lexical.pop().ok_or(Unsafe::Empty))?;
        let parent: PathBuf = lexical.iter().collect();
        let mut real = self.check(name, resolve(&self.root, &[], &parent, Kind::Entry))?;
        let dir = join(&self.root, &real);
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
        real.push(last);
        let path = join(&self.root, &real);
        Ok((real, path))
    }

    /// Returns the path for a file entry. The caller must not write through
    /// a link at that path: use [`create_file`] or `tar::Entry::unpack`.
    pub(super) fn file(&self, name: &Path) -> Result<PathBuf> {
        Ok(self.place(name)?.1)
    }

    /// Makes a symbolic link entry that points to `target`.
    pub(super) fn symlink(&mut self, name: &Path, target: &Path) -> Result<()> {
        let (real, path) = self.place(name)?;
        let parent = &real[..real.len() - 1];
        self.check(name, resolve(&self.root, parent, target, Kind::Link))?;
        remove_existing(&path)
            .and_then(|()| make_symlink(target, &path))
            .with_context(|| format!("Failed to create link: {}", path.display()))?;
        self.links.push((real, name.to_path_buf()));
        Ok(())
    }

    /// Makes a hard link entry. `target` is relative to the destination.
    pub(super) fn hard_link(&self, name: &Path, target: &Path) -> Result<()> {
        let src = self.check(name, resolve(&self.root, &[], target, Kind::Link))?;
        let src = join(&self.root, &src);
        if !fs::symlink_metadata(&src).is_ok_and(|meta| meta.is_file()) {
            return Err(anyhow!(
                "Failed to create hard link \"{}\" in archive {}: the target \"{}\" is not a file",
                name.display(),
                self.archive.display(),
                target.display()
            ));
        }
        let (_, path) = self.place(name)?;
        remove_existing(&path)
            .and_then(|()| fs::hard_link(&src, &path))
            .with_context(|| format!("Failed to create hard link: {}", path.display()))
    }

    /// Checks each link that this extraction made. A later link can change
    /// where an earlier link goes. This removes each link that goes out of
    /// the destination, until all links are safe, and returns the first error.
    pub(super) fn check_links(&self) -> Result<()> {
        let mut first = None;
        loop {
            let mut removed = false;
            for (real, name) in &self.links {
                let path = join(&self.root, real);
                // A later entry can replace the link.
                let Ok(target) = fs::read_link(&path) else {
                    continue;
                };
                let parent = &real[..real.len() - 1];
                if let Err(why) = resolve(&self.root, parent, &target, Kind::Link) {
                    fs::remove_file(&path)
                        .with_context(|| format!("Failed to remove link: {}", path.display()))?;
                    removed = true;
                    first.get_or_insert_with(|| self.unsafe_entry(name, why));
                }
            }
            if !removed {
                break;
            }
        }
        first.map_or(Ok(()), Err)
    }
}

/// Makes a new file at `path`. It removes a file or a link that is there,
/// and does not write through it.
pub(super) fn create_file(path: &Path) -> Result<File> {
    let open = || OpenOptions::new().write(true).create_new(true).open(path);
    open()
        .or_else(|err| {
            if err.kind() == io::ErrorKind::AlreadyExists {
                fs::remove_file(path).and_then(|()| open())
            } else {
                Err(err)
            }
        })
        .with_context(|| format!("Failed to create file: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(path: &str) -> Result<Vec<String>, Unsafe> {
        normalize(Path::new(path)).map(|parts| {
            parts
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        })
    }

    #[test]
    fn normalize_keeps_plain_names() {
        assert_eq!(norm("a/b.txt"), Ok(vec!["a".into(), "b.txt".into()]));
    }

    #[test]
    fn normalize_removes_dot_and_inner_dotdot() {
        assert_eq!(norm("./a/../b/./c"), Ok(vec!["b".into(), "c".into()]));
        assert_eq!(norm("a/.."), Ok(vec![]));
        assert_eq!(norm("."), Ok(vec![]));
    }

    #[test]
    fn normalize_rejects_escape() {
        assert_eq!(norm(".."), Err(Unsafe::Outside));
        assert_eq!(norm("a/../../b"), Err(Unsafe::Outside));
    }

    #[test]
    fn normalize_rejects_absolute() {
        assert_eq!(norm("/a"), Err(Unsafe::Absolute));
    }

    #[cfg(windows)]
    #[test]
    fn normalize_rejects_windows_prefix_and_stream() {
        assert_eq!(norm("C:\\a"), Err(Unsafe::Absolute));
        assert_eq!(norm("C:a"), Err(Unsafe::Absolute));
        assert_eq!(norm("\\\\server\\share\\a"), Err(Unsafe::Absolute));
        assert_eq!(norm("a.txt:stream"), Err(Unsafe::BadName));
    }

    #[test]
    fn every_reason_has_text() {
        for why in [
            Unsafe::Absolute,
            Unsafe::Outside,
            Unsafe::Empty,
            Unsafe::AbsoluteLink,
            Unsafe::LinkOutside,
            Unsafe::LinkLoop,
            Unsafe::BadName,
        ] {
            assert!(why.reason().starts_with("the "), "{why:?}");
        }
    }

    #[test]
    fn resolve_without_links_is_normalize() {
        let dir = tempfile::TempDir::new().unwrap();
        let got = resolve(dir.path(), &[], Path::new("a/../b"), Kind::Entry).unwrap();
        assert_eq!(got, vec![OsString::from("b")]);
        let err = resolve(
            dir.path(),
            &[OsString::from("a")],
            Path::new("../.."),
            Kind::Link,
        );
        assert_eq!(err, Err(Unsafe::LinkOutside));
        let err = resolve(dir.path(), &[], Path::new("/x"), Kind::Link);
        assert_eq!(err, Err(Unsafe::AbsoluteLink));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_follows_links_on_disk() {
        let dir = tempfile::TempDir::new().unwrap();
        std::os::unix::fs::symlink(".", dir.path().join("a")).unwrap();
        std::os::unix::fs::symlink("b", dir.path().join("loop_a")).unwrap();
        std::os::unix::fs::symlink("loop_a", dir.path().join("b")).unwrap();
        std::os::unix::fs::symlink("/", dir.path().join("abs")).unwrap();
        let root = dir.path();
        assert_eq!(
            resolve(root, &[], Path::new("a/a/x"), Kind::Entry),
            Ok(vec!["x".into()])
        );
        assert_eq!(
            resolve(root, &[], Path::new("a/.."), Kind::Entry),
            Err(Unsafe::Outside)
        );
        assert_eq!(
            resolve(root, &[], Path::new("loop_a"), Kind::Entry),
            Err(Unsafe::LinkLoop)
        );
        assert_eq!(
            resolve(root, &[], Path::new("abs/x"), Kind::Entry),
            Err(Unsafe::Outside)
        );
    }
}
