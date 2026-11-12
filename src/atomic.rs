//! Atomic file replace: write a temp file in the same directory, then
//! rename it to the target.
//!
//! `fs::write_atomic`, `fs::replace_in_file` and
//! `sh::CommandBuilder::stdout_atomic` use this module. It uses std only,
//! so `sh` adds no crate (ADR 0015).

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// The longest target name that goes into the temp file name, in bytes.
/// A longer name can make the temp name too long for the file system.
const MAX_NAME: usize = 64;

/// The number of names to try before an error.
const TRIES: u32 = 100;

/// A temp file that replaces its target on [`commit`](Self::commit).
/// A drop without a commit removes the temp file.
#[derive(Debug)]
pub struct Pending {
    /// The file to replace. Never a symlink: `new` resolves it.
    target: PathBuf,
    temp: PathBuf,
    /// `None` after the commit.
    file: Option<File>,
    committed: bool,
}

impl Pending {
    /// Makes an empty temp file in the directory of `path`.
    ///
    /// If `path` is a symlink, the target is the file that it points to.
    /// The link stays.
    pub fn new(path: &Path) -> io::Result<Self> {
        let target = resolve(path)?;
        let existing = match fs::metadata(&target) {
            Ok(meta) => Some(meta.permissions()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => return Err(err),
        };
        let (temp, file) = create_temp(parent(&target), target.file_name(), existing.as_ref())?;
        Ok(Self {
            target,
            temp,
            file: Some(file),
            committed: false,
        })
    }

    /// Gets the temp file to write.
    #[cfg(feature = "fs")]
    pub fn file_mut(&mut self) -> io::Result<&mut File> {
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("the temp file is closed"))
    }

    /// Gets the path of the temp file.
    #[cfg(feature = "sh")]
    pub fn temp_path(&self) -> &Path {
        &self.temp
    }

    /// Replaces the target with the temp file.
    ///
    /// Gives the temp file the permissions of the existing target, flushes
    /// it to disk, then renames it. On an error, the drop removes the temp
    /// file and the target stays.
    pub fn commit(mut self) -> io::Result<()> {
        let file = self
            .file
            .take()
            .ok_or_else(|| io::Error::other("the temp file is closed"))?;
        match fs::metadata(&self.target) {
            Ok(meta) => file.set_permissions(meta.permissions())?,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
        file.sync_all()?;
        // Close the file before the rename. Windows can refuse to rename an
        // open file.
        drop(file);
        fs::rename(&self.temp, &self.target)?;
        self.committed = true;
        sync_dir(parent(&self.target));
        Ok(())
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if !self.committed {
            // Close the file first, for Windows.
            drop(self.file.take());
            if fs::remove_file(&self.temp).is_err() {
                remove_read_only(&self.temp);
            }
        }
    }
}

/// Removes a read-only file. Windows can not remove it, and `commit` can
/// make the temp file read-only before a failed rename.
#[cfg(windows)]
fn remove_read_only(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_readonly(false);
        let _ = fs::set_permissions(path, perms);
        let _ = fs::remove_file(path);
    }
}

/// On Unix, a read-only file can be removed. Nothing to do.
#[cfg(not(windows))]
const fn remove_read_only(_path: &Path) {}

/// Replaces the file at `path` with the bytes that `fill` writes.
///
/// On an error, the target stays and no temp file stays.
#[cfg(feature = "fs")]
pub fn write(path: &Path, fill: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    let mut pending = Pending::new(path)?;
    fill(pending.file_mut()?)?;
    pending.commit()
}

/// Gets the file that `path` points to, if `path` is a symlink. Otherwise
/// gets `path`.
fn resolve(path: &Path) -> io::Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => fs::canonicalize(path),
        _ => Ok(path.to_path_buf()),
    }
}

/// Gets the directory that holds `path`: `.` for a bare name.
pub fn parent(path: &Path) -> &Path {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    }
}

/// Makes a new temp file in `dir`. The name starts with a dot and has
/// the target name, the process id, a counter and the time.
///
/// On Unix, the file gets the mode bits of `existing` less the umask, so
/// other users can not read new content of a private file. With no
/// `existing`, it gets the mode of `fs::write`: 0o666 less the umask.
fn create_temp(
    dir: &Path,
    name: Option<&OsStr>,
    existing: Option<&fs::Permissions>,
) -> io::Result<(PathBuf, File)> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let stem = name.filter(|n| n.len() <= MAX_NAME);
    for _ in 0..TRIES {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let mut file_name = OsString::from(".");
        file_name.push(stem.unwrap_or_else(|| OsStr::new("smop")));
        file_name.push(format!(".{}.{n}.{nanos:x}.tmp", std::process::id()));
        let path = dir.join(file_name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            options.mode(existing.map_or(0o666, |p| p.mode() & 0o777));
        }
        #[cfg(not(unix))]
        let _ = existing;
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free temp file name",
    ))
}

/// Flushes the directory entry after a rename. Unix only. Some file systems
/// do not support this, so this function ignores errors: the rename is
/// done.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(dir) = File::open(dir) {
        let _ = dir.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use tempfile::TempDir;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn parent_of_a_bare_name_is_the_current_directory() {
        assert_eq!(parent(Path::new("out.txt")), Path::new("."));
        assert_eq!(parent(Path::new("a/out.txt")), Path::new("a"));
    }

    #[test]
    fn temp_names_are_hidden_unique_and_in_the_same_directory() {
        let dir = TempDir::new().unwrap();
        let a = Pending::new(&dir.path().join("t.txt")).unwrap();
        let b = Pending::new(&dir.path().join("t.txt")).unwrap();
        assert_ne!(a.temp, b.temp);
        for p in [&a, &b] {
            assert_eq!(p.temp.parent(), Some(dir.path()));
            let name = p.temp.file_name().unwrap().to_string_lossy().into_owned();
            assert!(name.starts_with(".t.txt."), "{name}");
            assert!(
                Path::new(&name).extension() == Some(OsStr::new("tmp")),
                "{name}"
            );
        }
    }

    #[test]
    fn a_long_target_name_is_not_in_the_temp_name() {
        let dir = TempDir::new().unwrap();
        let long = "x".repeat(200);
        let p = Pending::new(&dir.path().join(&long)).unwrap();
        let name = p.temp.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with(".smop."), "{name}");
        drop(p);
        assert_eq!(names(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn drop_without_commit_removes_the_temp_file() {
        let dir = TempDir::new().unwrap();
        let p = Pending::new(&dir.path().join("t.txt")).unwrap();
        assert_eq!(names(dir.path()).len(), 1);
        drop(p);
        assert_eq!(names(dir.path()), Vec::<String>::new());
    }

    #[cfg(feature = "fs")]
    #[test]
    fn a_write_that_fails_midway_leaves_the_target_unchanged() {
        use std::io::Write;
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("t.txt");
        fs::write(&target, "old").unwrap();
        let err = write(&target, |file| {
            file.write_all(b"partial")?;
            Err(io::Error::other("disk full"))
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "disk full");
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
        assert_eq!(names(dir.path()), ["t.txt"]);
    }

    #[cfg(feature = "fs")]
    #[test]
    fn write_fills_the_target() {
        use std::io::Write;
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("t.txt");
        write(&target, |file| file.write_all(b"new")).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(names(dir.path()), ["t.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn the_temp_file_is_never_more_open_than_the_target() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("secret.env");
        fs::write(&target, "old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        let p = Pending::new(&target).unwrap();
        let mode = fs::metadata(&p.temp).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_is_an_error() {
        let dir = TempDir::new().unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink("missing", &link).unwrap();
        assert!(Pending::new(&link).is_err());
        assert_eq!(names(dir.path()), ["link"]);
    }
}
