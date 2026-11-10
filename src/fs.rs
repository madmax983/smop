//! File system utilities.
//!
//! Convenient functions for common file operations like reading and
//! writing, every one of which names the offending path in its error.
//!
//! Format helpers are behind their own features: `read_json`/`write_json`
//! (`json`), `read_toml`/`write_toml` (`toml`), `glob` (`glob`) and the CSV
//! functions (`csv`).

use std::fs::{self, OpenOptions};
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
#[cfg(any(feature = "json", feature = "toml", feature = "csv"))]
use serde::{Serialize, de::DeserializeOwned};

// Re-export TempDir for convenience
pub use tempfile::TempDir;

/// Reads a file's entire contents as a string.
///
/// # Errors
///
/// Returns an error if the file doesn't exist or can't be read.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// let content = fs::read_string("config.txt")?;
/// println!("{}", content);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read_string<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    fs::read_to_string(path).with_context(|| format!("Failed to read file: {}", path.display()))
}

/// Writes a string to a file, creating it if it doesn't exist.
///
/// Parent directories are NOT created automatically.
///
/// # Errors
///
/// Returns an error if the file can't be written.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// fs::write_string("output.txt", "Hello, world!")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn write_string<P: AsRef<Path>, C: AsRef<str>>(path: P, content: C) -> Result<()> {
    let path = path.as_ref();
    fs::write(path, content.as_ref())
        .with_context(|| format!("Failed to write file: {}", path.display()))
}

#[cfg(feature = "json")]
/// Reads and deserializes JSON from a file.
///
/// # Errors
///
/// Returns an error if the file doesn't exist or contains invalid JSON.
///
/// # Examples
///
/// ```no_run
/// use serde::Deserialize;
/// use smop::fs;
///
/// #[derive(Deserialize)]
/// struct Config {
///     name: String,
/// }
///
/// let config: Config = fs::read_json("config.json")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read_json<T: DeserializeOwned, P: AsRef<Path>>(path: P) -> Result<T> {
    let path = path.as_ref();
    let content = read_string(path)?;
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse JSON from: {}", path.display()))
}

#[cfg(feature = "json")]
/// Serializes and writes JSON to a file with pretty printing.
///
/// # Errors
///
/// Returns an error if the file can't be written.
///
/// # Examples
///
/// ```no_run
/// use serde::Serialize;
/// use smop::fs;
///
/// #[derive(Serialize)]
/// struct Config {
///     name: String,
/// }
///
/// let config = Config { name: "example".into() };
/// fs::write_json("config.json", &config)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn write_json<T: Serialize, P: AsRef<Path>>(path: P, value: &T) -> Result<()> {
    let path = path.as_ref();
    let content = serde_json::to_string_pretty(value)
        .with_context(|| format!("Failed to serialize JSON for: {}", path.display()))?;
    write_string(path, content)
}

/// Reads a file and returns its lines as a vector.
///
/// Empty lines are preserved. Line endings are stripped.
///
/// # Errors
///
/// Returns an error if the file doesn't exist or can't be read.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// let lines = fs::read_lines("data.txt")?;
/// for line in lines {
///     println!("{}", line);
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read_lines<P: AsRef<Path>>(path: P) -> Result<Vec<String>> {
    let content = read_string(path)?;
    Ok(content.lines().map(String::from).collect())
}

/// Appends content to a file, creating it if it doesn't exist.
///
/// # Errors
///
/// Returns an error if the file can't be written.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// fs::append("log.txt", "New log entry\n")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn append<P: AsRef<Path>, C: AsRef<str>>(path: P, content: C) -> Result<()> {
    let path = path.as_ref();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("Failed to open file for append: {}", path.display()))?;

    file.write_all(content.as_ref().as_bytes())
        .with_context(|| format!("Failed to append to file: {}", path.display()))
}

#[cfg(feature = "glob")]
/// Matches files using glob patterns and returns sorted paths.
///
/// # Errors
///
/// Returns an error if the pattern is invalid or filesystem access fails.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// let rs_files = fs::glob("src/**/*.rs")?;
/// for file in rs_files {
///     println!("{}", file.display());
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn glob<P: AsRef<str>>(pattern: P) -> Result<Vec<PathBuf>> {
    let pattern = pattern.as_ref();
    let mut paths: Vec<PathBuf> = glob::glob(pattern)
        .with_context(|| format!("Failed to parse glob pattern: {pattern}"))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("Failed to match glob pattern: {pattern}"))?;

    paths.sort();
    Ok(paths)
}

#[cfg(feature = "toml")]
/// Reads and deserializes TOML from a file.
///
/// # Errors
///
/// Returns an error if the file doesn't exist or contains invalid TOML.
///
/// # Examples
///
/// ```no_run
/// use serde::Deserialize;
/// use smop::fs;
///
/// #[derive(Deserialize)]
/// struct Config {
///     name: String,
/// }
///
/// let config: Config = fs::read_toml("config.toml")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read_toml<T: DeserializeOwned, P: AsRef<Path>>(path: P) -> Result<T> {
    let path = path.as_ref();
    let content = read_string(path)?;
    toml::from_str(&content)
        .with_context(|| format!("Failed to parse TOML from: {}", path.display()))
}

#[cfg(feature = "toml")]
/// Serializes and writes TOML to a file with pretty printing.
///
/// # Errors
///
/// Returns an error if the file can't be written.
///
/// # Examples
///
/// ```no_run
/// use serde::Serialize;
/// use smop::fs;
///
/// #[derive(Serialize)]
/// struct Config {
///     name: String,
/// }
///
/// let config = Config { name: "example".into() };
/// fs::write_toml("config.toml", &config)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn write_toml<T: Serialize, P: AsRef<Path>>(path: P, value: &T) -> Result<()> {
    let path = path.as_ref();
    let content = toml::to_string_pretty(value)
        .with_context(|| format!("Failed to serialize TOML for: {}", path.display()))?;
    write_string(path, content)
}

/// Creates a directory and all of its missing parents (like `mkdir -p`).
///
/// Succeeds if the directory already exists.
///
/// # Errors
///
/// Returns an error if the directory can't be created, for example because a
/// path component exists and is a file.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// fs::create_dir_all("target/generated/docs")?;
/// fs::write_string("target/generated/docs/index.md", "# Docs")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn create_dir_all<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    fs::create_dir_all(path)
        .with_context(|| format!("Failed to create directory: {}", path.display()))
}

/// Copies a file. An existing file at `to` is replaced.
///
/// This function copies files only, as Python's `shutil.copy` does. To copy
/// a directory, use [`copy_dir`].
///
/// # Errors
///
/// Returns an error that names the paths if `from` doesn't exist, is a
/// directory, or can't be copied.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// fs::copy("source.txt", "dest.txt")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    anyhow::ensure!(
        !from.is_dir(),
        "Failed to copy {} to {}: it is a directory; fs::copy copies files only, \
         use fs::copy_dir",
        from.display(),
        to.display()
    );
    fs::copy(from, to)
        .with_context(|| format!("Failed to copy {} to {}", from.display(), to.display()))?;
    Ok(())
}

/// Renames a file or directory.
///
/// # Errors
///
/// Returns an error if the source doesn't exist or the operation fails.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// fs::rename("old.txt", "new.txt")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    fs::rename(from, to)
        .with_context(|| format!("Failed to rename {} to {}", from.display(), to.display()))
}

/// Removes a file or directory (recursively if directory).
///
/// # Errors
///
/// Returns an error if the path doesn't exist or can't be removed.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// fs::remove("file.txt")?;
/// fs::remove("directory")?;  // Removes recursively
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn remove<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();

    // Try as file first, then as directory
    if let Err(e) = fs::remove_file(path) {
        if path.is_dir() {
            fs::remove_dir_all(path)
                .with_context(|| format!("Failed to remove directory: {}", path.display()))?;
        } else {
            return Err(e).with_context(|| format!("Failed to remove file: {}", path.display()));
        }
    }
    Ok(())
}

/// Reads a file's entire contents as bytes.
///
/// Use this for data that is not UTF-8 text.
///
/// # Errors
///
/// Returns an error that names the path if the file can't be read.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// let png = fs::read_bytes("logo.png")?;
/// println!("{} bytes", png.len());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn read_bytes<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
    let path = path.as_ref();
    fs::read(path).with_context(|| format!("Failed to read file: {}", path.display()))
}

/// Writes bytes to a file, creating it if it doesn't exist.
///
/// Parent directories are NOT created automatically.
///
/// # Errors
///
/// Returns an error that names the path if the file can't be written.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// fs::write_bytes("blob.bin", [0xde, 0xad, 0xbe, 0xef])?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn write_bytes<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, content: C) -> Result<()> {
    let path = path.as_ref();
    fs::write(path, content.as_ref())
        .with_context(|| format!("Failed to write file: {}", path.display()))
}

/// Copies a directory and all of its contents (like `cp -R` or Python's
/// `shutil.copytree`).
///
/// - `to` must not exist. This function does not merge into a directory.
/// - Empty directories are copied. Directory permissions are copied.
/// - Symlinks are copied as links. They are not followed. On Windows, this
///   needs the permission to make symlinks.
/// - If the copy fails, this function removes the part of `to` that it made.
///
/// To copy one file, use [`copy`].
///
/// # Errors
///
/// Returns an error that names the paths if:
/// - `from` is not a directory
/// - `to` exists, or is inside `from`
/// - an entry is not a file, a directory or a symlink (for example a socket)
/// - an entry can't be read or written
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// fs::copy_dir("assets", "target/dist/assets")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn copy_dir<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    copy_dir_inner(from, to).with_context(|| {
        format!(
            "Failed to copy directory {} to {}",
            from.display(),
            to.display()
        )
    })
}

fn copy_dir_inner(from: &Path, to: &Path) -> Result<()> {
    let meta = fs::metadata(from).with_context(|| format!("Failed to read {}", from.display()))?;
    anyhow::ensure!(
        meta.is_dir(),
        "{} is not a directory; use fs::copy to copy a file",
        from.display()
    );
    anyhow::ensure!(
        fs::symlink_metadata(to).is_err(),
        "{} already exists",
        to.display()
    );
    anyhow::ensure!(
        !is_inside(from, to),
        "{} is inside {}",
        to.display(),
        from.display()
    );

    fs::create_dir(to).with_context(|| format!("Failed to create directory: {}", to.display()))?;
    // Set directory permissions last, deepest first: a read-only directory
    // must not stop the copy or the clean-up.
    let mut dirs = vec![(to.to_path_buf(), meta.permissions())];
    let result = copy_dir_contents(from, to, &mut dirs).and_then(|()| {
        for (dir, perms) in dirs.iter().rev() {
            fs::set_permissions(dir, perms.clone())
                .with_context(|| format!("Failed to set permissions: {}", dir.display()))?;
        }
        Ok(())
    });
    if result.is_err() {
        // Best effort: the original error is the one to report.
        let _ = fs::remove_dir_all(to);
    }
    result
}

/// The directory that holds `path`: `.` for a bare file name.
fn parent_dir(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

/// True if `to` (which does not exist yet) would be inside `from`.
fn is_inside(from: &Path, to: &Path) -> bool {
    let (Ok(from), Ok(parent), Some(name)) = (
        fs::canonicalize(from),
        fs::canonicalize(parent_dir(to)),
        to.file_name(),
    ) else {
        // Can't tell. `create_dir` reports a missing parent.
        return false;
    };
    parent.join(name).starts_with(from)
}

fn copy_dir_contents(
    from: &Path,
    to: &Path,
    dirs: &mut Vec<(PathBuf, fs::Permissions)>,
) -> Result<()> {
    let mut stack = vec![(from.to_path_buf(), to.to_path_buf())];
    while let Some((src_dir, dst_dir)) = stack.pop() {
        for entry in read_dir_entries(&src_dir)? {
            let src = entry.path();
            let dst = dst_dir.join(entry.file_name());
            let file_type = entry
                .file_type()
                .with_context(|| format!("Failed to read file type: {}", src.display()))?;
            if file_type.is_dir() {
                let perms = fs::symlink_metadata(&src)
                    .with_context(|| format!("Failed to read {}", src.display()))?
                    .permissions();
                fs::create_dir(&dst)
                    .with_context(|| format!("Failed to create directory: {}", dst.display()))?;
                dirs.push((dst.clone(), perms));
                stack.push((src, dst));
            } else {
                copy_entry(&src, &dst, file_type)?;
            }
        }
    }
    Ok(())
}

/// Reads a directory's entries. Errors name the directory.
fn read_dir_entries(dir: &Path) -> Result<Vec<fs::DirEntry>> {
    let context = || format!("Failed to read directory: {}", dir.display());
    fs::read_dir(dir)
        .with_context(context)?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(context)
}

/// Copies a file, or a symlink as a link, to `to`, which must not exist.
/// Refuses other types: `std::fs::copy` blocks forever on a FIFO.
fn copy_entry(from: &Path, to: &Path, file_type: fs::FileType) -> Result<()> {
    if file_type.is_symlink() {
        let target = fs::read_link(from)
            .with_context(|| format!("Failed to read symlink: {}", from.display()))?;
        make_symlink(&target, to, file_type).with_context(|| {
            format!(
                "Failed to copy symlink {} to {}",
                from.display(),
                to.display()
            )
        })
    } else if file_type.is_file() {
        fs::copy(from, to)
            .map(drop)
            .with_context(|| format!("Failed to copy {} to {}", from.display(), to.display()))
    } else {
        anyhow::bail!(
            "Can't copy {}: it is not a file, a directory or a symlink",
            from.display()
        )
    }
}

#[cfg(unix)]
fn make_symlink(target: &Path, link: &Path, _file_type: fs::FileType) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn make_symlink(target: &Path, link: &Path, file_type: fs::FileType) -> std::io::Result<()> {
    use std::os::windows::fs::{FileTypeExt, symlink_dir, symlink_file};
    if file_type.is_symlink_dir() {
        symlink_dir(target, link)
    } else {
        symlink_file(target, link)
    }
}

#[cfg(not(any(unix, windows)))]
fn make_symlink(_target: &Path, _link: &Path, _file_type: fs::FileType) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "this platform has no symlinks",
    ))
}

/// Removes one entry of the given type. A directory is removed with its
/// contents. A symlink is removed, not its target.
fn remove_entry(path: &Path, file_type: fs::FileType) -> std::io::Result<()> {
    if file_type.is_dir() {
        return fs::remove_dir_all(path);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        if file_type.is_symlink_dir() {
            return fs::remove_dir(path);
        }
    }
    fs::remove_file(path)
}

/// Moves a file, directory or symlink to a new path (like `mv` or Python's
/// `shutil.move`).
///
/// This function first tries [`rename`]. If `from` and `to` are on
/// different file systems, it copies `from` to `to` and then removes `from`.
///
/// - `to` is the new path, not a directory to move into.
/// - If `from` is a file or a symlink, an existing file at `to` is replaced.
/// - If `from` is a directory, `to` must not exist.
/// - If the copy fails, this function removes the partial copy and keeps
///   `from`. If the copy is complete but `from` can't be removed, both stay
///   and the error tells you.
///
/// # Errors
///
/// Returns an error that names both paths if `from` doesn't exist, `to`
/// can't be used, or the move fails.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// // Works when /tmp is tmpfs and target/ is on disk.
/// fs::move_path("/tmp/build-output", "target/dist")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn move_path<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<()> {
    move_path_with(from.as_ref(), to.as_ref(), |from, to| fs::rename(from, to))
}

/// [`move_path`] with the rename step as a parameter, so tests can force the
/// cross-device fallback.
fn move_path_with(
    from: &Path,
    to: &Path,
    rename: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<()> {
    let context = || format!("Failed to move {} to {}", from.display(), to.display());
    let meta = fs::symlink_metadata(from).with_context(context)?;
    if let Ok(existing) = fs::symlink_metadata(to) {
        anyhow::ensure!(
            !meta.is_dir() && !existing.is_dir(),
            "{}: {} already exists",
            context(),
            to.display()
        );
    }
    match rename(from, to) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::CrossesDevices => {
            copy_then_remove(from, to, meta.file_type()).with_context(context)
        }
        Err(err) => Err(err).with_context(context),
    }
}

/// The cross-device half of [`move_path`].
fn copy_then_remove(from: &Path, to: &Path, file_type: fs::FileType) -> Result<()> {
    if file_type.is_dir() {
        copy_dir(from, to)?;
    } else {
        // Copy next to `to`, then rename into place. A failed copy leaves
        // nothing behind (the temp path is removed on drop), and an existing
        // file at `to` is replaced in one step.
        let parent = parent_dir(to);
        let mut copy_error = None;
        let temp = tempfile::Builder::new()
            .prefix(".smop-move")
            .make_in(parent, |temp| {
                // `make_in` tries a new name on `AlreadyExists`, so only that
                // error goes back to it. Keep the full error for the report.
                match copy_entry(from, temp, file_type) {
                    Ok(()) => Ok(()),
                    Err(err) => {
                        copy_error = Some(err);
                        Err(std::io::Error::other("copy failed"))
                    }
                }
            });
        let temp = match (temp, copy_error) {
            (_, Some(err)) => return Err(err),
            (Err(err), None) => {
                return Err(err).with_context(|| {
                    format!("Failed to make a temp file in {}", parent.display())
                });
            }
            (Ok(temp), None) => temp,
        };
        temp.persist(to)
            .map_err(|err| err.error)
            .with_context(|| format!("Failed to rename the copy to {}", to.display()))?;
    }
    remove_entry(from, file_type).with_context(|| {
        format!(
            "Copied {} to {}, but failed to remove {}; both now exist",
            from.display(),
            to.display(),
            from.display()
        )
    })
}

/// Removes a file or directory if it exists. A missing path is not an error.
///
/// A directory is removed with all of its contents. A symlink is removed, not
/// its target.
///
/// # Errors
///
/// Returns an error that names the path for all failures other than a
/// missing path, for example a permission error.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// fs::remove_if_exists("target/dist")?; // Ok if it isn't there
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn remove_if_exists<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    let context = || format!("Failed to remove: {}", path.display());
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err).with_context(context),
    };
    match remove_entry(path, meta.file_type()) {
        // Something else removed it first.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result.with_context(context),
    }
}

/// Lists all files below a directory, recursively, sorted by path (like
/// `find dir -type f | sort` or Python's `os.walk`).
///
/// - Directories are not in the list, but their files are.
/// - Symlinks to directories are not followed and are not in the list, so a
///   symlink cycle can't make a loop. Other symlinks are in the list.
/// - Each path starts with `dir`.
///
/// # Errors
///
/// Returns an error that names the path if `dir` is not a directory, or if
/// a directory below it can't be read.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// for file in fs::walk("src")? {
///     if file.extension().is_some_and(|ext| ext == "rs") {
///         println!("{}", file.display());
///     }
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn walk<P: AsRef<Path>>(dir: P) -> Result<Vec<PathBuf>> {
    let root = dir.as_ref();
    let context = || format!("Failed to walk directory: {}", root.display());
    let meta = fs::metadata(root).with_context(context)?;
    anyhow::ensure!(meta.is_dir(), "{}: it is not a directory", context());

    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in read_dir_entries(&dir)? {
            let path = entry.path();
            let file_type = entry
                .file_type()
                .with_context(|| format!("Failed to read file type: {}", path.display()))?;
            if file_type.is_dir() {
                stack.push(path);
            } else if !(file_type.is_symlink() && path.is_dir()) {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Creates a temporary file and returns a handle and path.
///
/// The file will be deleted when the handle is dropped unless persisted.
///
/// # Errors
///
/// Returns an error if the temporary file can't be created.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// let (mut file, path) = fs::temp_file()?;
/// use std::io::Write;
/// writeln!(file, "temporary data")?;
/// println!("Temp file at: {}", path.display());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn temp_file() -> Result<(std::fs::File, PathBuf)> {
    let file = tempfile::NamedTempFile::new().context("Failed to create temporary file")?;
    let path = file.path().to_path_buf();
    let file = file.into_file();
    Ok((file, path))
}

/// Creates a temporary directory that will be auto-cleaned on drop.
///
/// # Errors
///
/// Returns an error if the temporary directory can't be created.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// let temp = fs::temp_dir()?;
/// let file_path = temp.path().join("test.txt");
/// fs::write_string(&file_path, "data")?;
/// // Directory automatically cleaned up when `temp` goes out of scope
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn temp_dir() -> Result<TempDir> {
    TempDir::new().context("Failed to create temporary directory")
}

// ============================================================================
// File locks
// ============================================================================

/// How often [`lock`] and [`lock_shared`] retry while another holder has the
/// lock. std has no blocking lock with a timeout, so waiting means polling.
const LOCK_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

/// A lock on a file (or, on Unix, a directory), from [`lock`] or
/// [`lock_shared`]. See [`lock`] for how it behaves on each platform.
///
/// The lock is released when this guard is dropped.
#[derive(Debug)]
#[must_use = "the lock is released as soon as the guard is dropped"]
pub struct FileLock {
    file: fs::File,
    path: PathBuf,
    shared: bool,
}

impl FileLock {
    /// The path that is locked.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether this is a shared lock ([`lock_shared`]) rather than an
    /// exclusive one ([`lock`]).
    #[must_use]
    pub const fn is_shared(&self) -> bool {
        self.shared
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // Closing the file would release it too; unlocking first makes the
        // release explicit. A failure here leaves nothing to do.
        let _ = self.file.unlock();
    }
}

/// Takes an exclusive lock on `path`, waiting up to `timeout` for other
/// holders to let go.
///
/// On Linux, macOS, FreeBSD and NetBSD, std implements this with `flock(2)`:
/// it is the lock `flock -x` takes, so it works together with `flock(1)` and
/// other programs using `flock(2)` on the same path. On Windows it is a
/// `LockFileEx` lock. Other Unix targets depend on your Rust version's std;
/// where it doesn't support file locks (Android, for example), this always
/// fails with an error.
///
/// - `path` may be a file or, on Unix, a directory. Like `flock(1)`, a missing
///   path is created as an empty file (its parent directory must exist), so
///   create a directory first (e.g. with [`create_dir_all`]) if that is what
///   you mean to lock.
/// - While another process (or another handle in this one) holds the lock,
///   this retries every 50 ms until `timeout` passes. `Duration::ZERO` tries
///   once; `Duration::MAX` waits indefinitely.
/// - The lock is released when the returned [`FileLock`] is dropped, or when
///   the process exits.
///
/// On Unix the lock is advisory: it only excludes other code that also locks
/// the path (network filesystems such as CIFS may enforce it). On Windows it is
/// mandatory: while an exclusive lock is held,
/// other handles can neither read nor write the file (a shared lock still lets
/// them read). Lock a dedicated lock file, as below, rather than a file you
/// also read or write, and the code behaves the same on both.
///
/// # Errors
///
/// Returns an error naming the path if it can't be opened or locked, or if
/// another holder still has the lock when `timeout` runs out. On Windows,
/// locking a directory is an error: lock a dedicated lock file next to it
/// instead (`target/dist.lock` for `target/dist`, as in the example).
///
/// # Examples
///
/// ```no_run
/// use std::time::Duration;
/// use smop::fs;
///
/// // Wait up to a minute for a concurrent release to finish, as `flock -w 60 -x` does.
/// let _guard = fs::lock("target/dist.lock", Duration::from_secs(60))?;
/// fs::write_string("target/dist/VERSION", "1.2.3")?;
/// // Released here, when `_guard` goes out of scope.
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn lock<P: AsRef<Path>>(path: P, timeout: std::time::Duration) -> Result<FileLock> {
    acquire_lock(path.as_ref(), timeout, false)
}

/// Takes a shared lock on `path`, waiting up to `timeout` for an exclusive
/// holder to let go.
///
/// Any number of shared locks can be held at once; an exclusive [`lock`]
/// waits for all of them. Where std uses `flock(2)` (see [`lock`]), this is
/// the lock `flock -s` takes. Paths, timeouts and release work as in [`lock`].
///
/// # Errors
///
/// Returns an error naming the path if it can't be opened or locked, or if an
/// exclusive holder still has the lock when `timeout` runs out.
///
/// # Examples
///
/// ```no_run
/// use std::time::Duration;
/// use smop::fs;
///
/// let _reading = fs::lock_shared("target/dist.lock", Duration::from_secs(5))?;
/// let version = fs::read_string("target/dist/VERSION")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn lock_shared<P: AsRef<Path>>(path: P, timeout: std::time::Duration) -> Result<FileLock> {
    acquire_lock(path.as_ref(), timeout, true)
}

fn acquire_lock(path: &Path, timeout: std::time::Duration, shared: bool) -> Result<FileLock> {
    use std::time::Instant;

    // With its article, for messages: "an exclusive lock", "a shared lock".
    let kind = if shared { "a shared" } else { "an exclusive" };

    #[cfg(windows)]
    if path.is_dir() {
        anyhow::bail!(
            "Can't take {kind} lock on directory {}: Windows can't lock directories; \
             lock a dedicated lock file next to it instead",
            path.display()
        );
    }

    let file = open_for_lock(path, shared)
        .with_context(|| format!("Failed to open {} for {kind} lock", path.display()))?;

    // `None` (an overflowing deadline, e.g. Duration::MAX) means wait forever.
    let deadline = Instant::now().checked_add(timeout);
    loop {
        let attempt = if shared {
            file.try_lock_shared()
        } else {
            file.try_lock()
        };
        match attempt {
            Ok(()) => {
                return Ok(FileLock {
                    file,
                    path: path.to_path_buf(),
                    shared,
                });
            }
            Err(fs::TryLockError::WouldBlock) => {}
            Err(fs::TryLockError::Error(err)) => {
                return Err(err)
                    .with_context(|| format!("Failed to take {kind} lock on {}", path.display()));
            }
        }

        let wait = match deadline {
            Some(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    anyhow::bail!(
                        "Timed out after {timeout:?} waiting for {kind} lock on {} \
                         (another holder still has it)",
                        path.display()
                    );
                }
                left.min(LOCK_POLL_INTERVAL)
            }
            None => LOCK_POLL_INTERVAL,
        };
        std::thread::sleep(wait);
    }
}

/// Opens `path` for locking, creating it as an empty file if it doesn't exist
/// (as `flock(1)` does).
///
/// An exclusive lock gets a writable handle when one can be opened: std leaves
/// exclusive locks on read-only handles unspecified, and NFS, which emulates
/// `flock(2)` with `fcntl(2)` locks, rejects them (EBADF). `flock(1)` reopens
/// read-write for the same reason. Paths that can't be opened for writing (a
/// directory, a read-only file or filesystem) fall back to a read-only handle,
/// which local filesystems and `LockFileEx` accept.
fn open_for_lock(path: &Path, shared: bool) -> std::io::Result<fs::File> {
    if !shared
        && let Ok(file) = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
    {
        return Ok(file);
    }
    match fs::File::open(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path),
        result => result,
    }
}

// ============================================================================
// CSV Functions (feature-gated)
// ============================================================================

/// Reads and deserializes CSV from a file.
///
/// Each row is deserialized into the specified type. The first row is treated
/// as headers and used for field matching.
///
/// This function is only available with the `csv` feature.
///
/// # Errors
///
/// Returns an error if the file doesn't exist or contains invalid CSV.
///
/// # Examples
///
/// ```no_run
/// use serde::Deserialize;
/// use smop::fs;
///
/// #[derive(Deserialize)]
/// struct Record {
///     name: String,
///     age: u32,
/// }
///
/// let records: Vec<Record> = fs::read_csv("data.csv")?;
/// for record in records {
///     println!("{}: {}", record.name, record.age);
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
#[cfg(feature = "csv")]
pub fn read_csv<T: DeserializeOwned, P: AsRef<Path>>(path: P) -> Result<Vec<T>> {
    let path = path.as_ref();
    let mut reader = csv::Reader::from_path(path)
        .with_context(|| format!("Failed to open CSV file: {}", path.display()))?;

    let mut records = Vec::new();
    for result in reader.deserialize() {
        let record: T = result
            .with_context(|| format!("Failed to parse CSV record from: {}", path.display()))?;
        records.push(record);
    }
    Ok(records)
}

/// Serializes and writes records to a CSV file.
///
/// The first row will contain headers derived from the struct field names.
///
/// This function is only available with the `csv` feature.
///
/// # Errors
///
/// Returns an error if the file can't be written.
///
/// # Examples
///
/// ```no_run
/// use serde::Serialize;
/// use smop::fs;
///
/// #[derive(Serialize)]
/// struct Record {
///     name: String,
///     age: u32,
/// }
///
/// let records = vec![
///     Record { name: "Alice".into(), age: 30 },
///     Record { name: "Bob".into(), age: 25 },
/// ];
/// fs::write_csv("output.csv", &records)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[cfg(feature = "csv")]
pub fn write_csv<T: Serialize, P: AsRef<Path>>(path: P, records: &[T]) -> Result<()> {
    let path = path.as_ref();
    let mut writer = csv::Writer::from_path(path)
        .with_context(|| format!("Failed to create CSV file: {}", path.display()))?;

    for record in records {
        writer
            .serialize(record)
            .with_context(|| format!("Failed to write CSV record to: {}", path.display()))?;
    }
    writer
        .flush()
        .with_context(|| format!("Failed to flush CSV file: {}", path.display()))?;
    Ok(())
}

/// Reads CSV as raw string rows without type deserialization.
///
/// Returns each row as a vector of strings. The first row (headers) is included.
///
/// This function is only available with the `csv` feature.
///
/// # Errors
///
/// Returns an error if the file doesn't exist or can't be parsed.
///
/// # Examples
///
/// ```no_run
/// use smop::fs;
///
/// let rows = fs::read_csv_rows("data.csv")?;
/// for row in rows {
///     println!("{:?}", row);
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
#[cfg(feature = "csv")]
pub fn read_csv_rows<P: AsRef<Path>>(path: P) -> Result<Vec<Vec<String>>> {
    let path = path.as_ref();
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_path(path)
        .with_context(|| format!("Failed to open CSV file: {}", path.display()))?;

    let mut rows = Vec::new();
    for result in reader.records() {
        let record =
            result.with_context(|| format!("Failed to parse CSV row from: {}", path.display()))?;
        rows.push(record.iter().map(String::from).collect());
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn read_string_reads_file_contents() {
        let dir = setup();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "Hello, world!").unwrap();

        let content = read_string(&path).unwrap();
        assert_eq!(content, "Hello, world!");
    }

    #[test]
    fn read_string_fails_on_missing_file() {
        let result = read_string("/nonexistent/path/file.txt");
        assert!(result.is_err());
    }

    #[test]
    fn write_string_creates_file() {
        let dir = setup();
        let path = dir.path().join("new.txt");

        write_string(&path, "New content").unwrap();

        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "New content");
    }

    #[test]
    fn write_string_overwrites_existing() {
        let dir = setup();
        let path = dir.path().join("existing.txt");
        std::fs::write(&path, "Old").unwrap();

        write_string(&path, "New").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "New");
    }

    #[cfg(feature = "json")]
    #[test]
    fn read_json_deserializes_correctly() {
        let dir = setup();
        let path = dir.path().join("data.json");
        std::fs::write(&path, r#"{"name": "test", "value": 42}"#).unwrap();

        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct Data {
            name: String,
            value: i32,
        }

        let data: Data = read_json(&path).unwrap();
        assert_eq!(data.name, "test");
        assert_eq!(data.value, 42);
    }

    #[cfg(feature = "json")]
    #[test]
    fn read_json_fails_on_invalid_json() {
        let dir = setup();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not valid json").unwrap();

        let result: Result<serde_json::Value> = read_json(&path);
        assert!(result.is_err());
    }

    #[cfg(feature = "json")]
    #[test]
    fn write_json_serializes_with_pretty_print() {
        let dir = setup();
        let path = dir.path().join("out.json");

        #[derive(serde::Serialize)]
        struct Data {
            name: String,
        }

        let data = Data {
            name: "test".to_string(),
        };
        write_json(&path, &data).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains('\n'), "Should be pretty-printed");
        assert!(content.contains("\"name\": \"test\""));
    }

    #[test]
    fn read_lines_splits_on_newlines() {
        let dir = setup();
        let path = dir.path().join("lines.txt");
        std::fs::write(&path, "line1\nline2\nline3").unwrap();

        let lines = read_lines(&path).unwrap();
        assert_eq!(lines, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn read_lines_preserves_empty_lines() {
        let dir = setup();
        let path = dir.path().join("empty.txt");
        std::fs::write(&path, "line1\n\nline3").unwrap();

        let lines = read_lines(&path).unwrap();
        assert_eq!(lines, vec!["line1", "", "line3"]);
    }

    #[test]
    fn append_creates_file_if_missing() {
        let dir = setup();
        let path = dir.path().join("append.txt");

        append(&path, "first").unwrap();

        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");
    }

    #[test]
    fn append_adds_to_existing_file() {
        let dir = setup();
        let path = dir.path().join("append2.txt");
        std::fs::write(&path, "existing\n").unwrap();

        append(&path, "new\n").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "existing\nnew\n");
    }

    // CSV tests (feature-gated)

    #[cfg(feature = "csv")]
    #[test]
    fn read_csv_deserializes_records() {
        let dir = setup();
        let path = dir.path().join("data.csv");
        std::fs::write(&path, "name,age\nAlice,30\nBob,25").unwrap();

        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct Record {
            name: String,
            age: u32,
        }

        let records: Vec<Record> = read_csv(&path).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].name, "Alice");
        assert_eq!(records[0].age, 30);
        assert_eq!(records[1].name, "Bob");
        assert_eq!(records[1].age, 25);
    }

    #[cfg(feature = "csv")]
    #[test]
    fn read_csv_fails_on_missing_file() {
        let result: Result<Vec<std::collections::HashMap<String, String>>> =
            read_csv("/nonexistent/path/file.csv");
        assert!(result.is_err());
    }

    #[cfg(feature = "csv")]
    #[test]
    fn write_csv_creates_file_with_headers() {
        let dir = setup();
        let path = dir.path().join("output.csv");

        #[derive(serde::Serialize)]
        struct Record {
            name: String,
            score: i32,
        }

        let records = vec![
            Record {
                name: "Alice".to_string(),
                score: 95,
            },
            Record {
                name: "Bob".to_string(),
                score: 87,
            },
        ];
        write_csv(&path, &records).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("name,score"));
        assert!(content.contains("Alice,95"));
        assert!(content.contains("Bob,87"));
    }

    #[cfg(feature = "csv")]
    #[test]
    fn read_csv_rows_returns_all_rows() {
        let dir = setup();
        let path = dir.path().join("raw.csv");
        std::fs::write(&path, "a,b,c\n1,2,3\n4,5,6").unwrap();

        let rows = read_csv_rows(&path).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], vec!["a", "b", "c"]);
        assert_eq!(rows[1], vec!["1", "2", "3"]);
        assert_eq!(rows[2], vec!["4", "5", "6"]);
    }

    #[cfg(feature = "csv")]
    #[test]
    fn read_csv_roundtrip() {
        let dir = setup();
        let path = dir.path().join("roundtrip.csv");

        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Record {
            id: u32,
            value: String,
        }

        let original = vec![
            Record {
                id: 1,
                value: "first".to_string(),
            },
            Record {
                id: 2,
                value: "second".to_string(),
            },
        ];

        write_csv(&path, &original).unwrap();
        let loaded: Vec<Record> = read_csv(&path).unwrap();

        assert_eq!(original, loaded);
    }

    #[cfg(feature = "glob")]
    #[test]
    fn glob_matches_files() {
        let dir = setup();
        std::fs::write(dir.path().join("test1.txt"), "a").unwrap();
        std::fs::write(dir.path().join("test2.txt"), "b").unwrap();
        std::fs::write(dir.path().join("other.rs"), "c").unwrap();

        let pattern = format!("{}/**/*.txt", dir.path().display());
        let matches = glob(&pattern).unwrap();

        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|p| p.extension().unwrap() == "txt"));
    }

    #[cfg(feature = "glob")]
    #[test]
    fn glob_returns_sorted_results() {
        let dir = setup();
        std::fs::write(dir.path().join("c.txt"), "").unwrap();
        std::fs::write(dir.path().join("a.txt"), "").unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();

        let pattern = format!("{}/*.txt", dir.path().display());
        let matches = glob(&pattern).unwrap();

        let names: Vec<_> = matches
            .iter()
            .filter_map(|p| p.file_name()?.to_str())
            .collect();

        assert_eq!(names, vec!["a.txt", "b.txt", "c.txt"]);
    }

    #[cfg(feature = "toml")]
    #[test]
    fn read_toml_deserializes_correctly() {
        let dir = setup();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "name = \"test\"\nvalue = 42").unwrap();

        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct Config {
            name: String,
            value: i32,
        }

        let config: Config = read_toml(&path).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.value, 42);
    }

    #[cfg(feature = "toml")]
    #[test]
    fn write_toml_serializes_correctly() {
        let dir = setup();
        let path = dir.path().join("output.toml");

        #[derive(serde::Serialize)]
        struct Config {
            name: String,
            value: i32,
        }

        let config = Config {
            name: "test".to_string(),
            value: 42,
        };
        write_toml(&path, &config).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("name = \"test\""));
        assert!(content.contains("value = 42"));
    }

    #[cfg(feature = "toml")]
    #[test]
    fn toml_roundtrip() {
        let dir = setup();
        let path = dir.path().join("roundtrip.toml");

        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Config {
            name: String,
            count: u32,
        }

        let original = Config {
            name: "test".to_string(),
            count: 123,
        };

        write_toml(&path, &original).unwrap();
        let loaded: Config = read_toml(&path).unwrap();

        assert_eq!(original, loaded);
    }

    #[test]
    fn copy_file_works() {
        let dir = setup();
        let src = dir.path().join("source.txt");
        let dst = dir.path().join("dest.txt");

        std::fs::write(&src, "content").unwrap();
        copy(&src, &dst).unwrap();

        assert!(dst.exists());
        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "content");
    }

    #[test]
    fn rename_file_works() {
        let dir = setup();
        let old = dir.path().join("old.txt");
        let new = dir.path().join("new.txt");

        std::fs::write(&old, "content").unwrap();
        rename(&old, &new).unwrap();

        assert!(!old.exists());
        assert!(new.exists());
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "content");
    }

    #[test]
    fn remove_file_works() {
        let dir = setup();
        let path = dir.path().join("remove_me.txt");

        std::fs::write(&path, "content").unwrap();
        assert!(path.exists());

        remove(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn remove_directory_works() {
        let dir = setup();
        let subdir = dir.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();
        std::fs::write(subdir.join("file.txt"), "content").unwrap();

        remove(&subdir).unwrap();
        assert!(!subdir.exists());
    }

    #[test]
    fn temp_file_creates_file() {
        let (file, path) = temp_file().unwrap();
        drop(file);
        // Note: Path may or may not exist after drop depending on tempfile behavior
        // The important thing is we got a valid path
        assert!(!path.as_os_str().is_empty());
    }

    #[test]
    fn temp_dir_creates_directory() {
        let temp = temp_dir().unwrap();
        assert!(temp.path().exists());
        assert!(temp.path().is_dir());

        // Write a file into it
        let file_path = temp.path().join("test.txt");
        std::fs::write(&file_path, "data").unwrap();
        assert!(file_path.exists());

        // When temp drops, directory should be cleaned up
    }

    #[test]
    fn create_dir_all_creates_nested_directories() {
        let dir = setup();
        let nested = dir.path().join("a/b/c");
        create_dir_all(&nested).unwrap();
        assert!(nested.is_dir());
    }

    #[test]
    fn create_dir_all_is_ok_when_directory_exists() {
        let dir = setup();
        create_dir_all(dir.path()).unwrap();
        create_dir_all(dir.path()).unwrap();
    }

    #[test]
    fn create_dir_all_error_names_path() {
        let dir = setup();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();
        let err = create_dir_all(file.join("sub")).unwrap_err();
        assert!(
            err.to_string().contains("file.txt"),
            "error should name the path: {err}"
        );
    }

    #[test]
    fn read_string_error_names_path() {
        let err = read_string("/nonexistent/smop/path.txt").unwrap_err();
        assert!(
            err.to_string().contains("/nonexistent/smop/path.txt"),
            "{err}"
        );
    }

    // ------------------------------------------------------------------
    // File locks
    // ------------------------------------------------------------------

    use std::time::{Duration, Instant};

    fn lock_file(dir: &TempDir) -> PathBuf {
        let path = dir.path().join("release.lock");
        std::fs::write(&path, "").unwrap();
        path
    }

    #[test]
    fn lock_is_exclusive_until_dropped() {
        let dir = setup();
        let path = lock_file(&dir);
        let guard = lock(&path, Duration::from_secs(2)).unwrap();
        assert_eq!(guard.path(), path);
        assert!(!guard.is_shared());

        let err = lock(&path, Duration::ZERO).unwrap_err().to_string();
        assert!(
            err.contains("Timed out") && err.contains("exclusive"),
            "{err}"
        );

        drop(guard);
        let _again = lock(&path, Duration::ZERO).unwrap();
    }

    #[test]
    fn lock_timeout_error_names_path_and_timeout() {
        let dir = setup();
        let path = lock_file(&dir);
        let _held = lock(&path, Duration::ZERO).unwrap();

        let start = Instant::now();
        let err = lock(&path, Duration::from_millis(120))
            .unwrap_err()
            .to_string();
        let waited = start.elapsed();

        assert!(err.contains(&path.display().to_string()), "{err}");
        assert!(err.contains("120ms"), "{err}");
        assert!(
            waited >= Duration::from_millis(120),
            "gave up early: {waited:?}"
        );
        assert!(waited < Duration::from_secs(2), "overslept: {waited:?}");
    }

    #[test]
    fn lock_waits_for_the_holder_to_let_go() {
        let dir = setup();
        let path = lock_file(&dir);
        let held = lock(&path, Duration::ZERO).unwrap();
        // Start before spawning: the release can't come before start + 150 ms,
        // however the threads are scheduled.
        let start = Instant::now();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            drop(held);
        });

        let guard = lock(&path, Duration::from_secs(5)).unwrap();
        let waited = start.elapsed();
        assert!(
            waited >= Duration::from_millis(100),
            "got it early: {waited:?}"
        );
        // Polling (every 50 ms) picks up the release at ~150 ms; sleeping out
        // the 5 s timeout and trying once more would not.
        assert!(waited < Duration::from_secs(2), "didn't poll: {waited:?}");
        releaser.join().unwrap();
        drop(guard);
    }

    #[test]
    fn lock_with_duration_max_waits_without_a_deadline() {
        let dir = setup();
        let path = lock_file(&dir);
        let held = lock(&path, Duration::ZERO).unwrap();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(held);
        });

        // Instant::now() + Duration::MAX overflows; that must mean "no
        // deadline", not a panic or an immediate timeout.
        let _guard = lock(&path, Duration::MAX).unwrap();
        releaser.join().unwrap();
    }

    #[test]
    fn shared_locks_coexist_and_exclude_exclusive_ones() {
        let dir = setup();
        let path = lock_file(&dir);

        let first = lock_shared(&path, Duration::ZERO).unwrap();
        let second = lock_shared(&path, Duration::ZERO).unwrap();
        assert!(first.is_shared() && second.is_shared());
        let err = lock(&path, Duration::ZERO).unwrap_err().to_string();
        assert!(err.contains("exclusive"), "{err}");

        drop((first, second));
        let exclusive = lock(&path, Duration::ZERO).unwrap();
        let err = lock_shared(&path, Duration::ZERO).unwrap_err().to_string();
        assert!(err.contains("shared") && err.contains("Timed out"), "{err}");
        drop(exclusive);
    }

    #[test]
    fn lock_creates_a_missing_file_like_flock() {
        let dir = setup();
        let path = dir.path().join("new.lock");
        let guard = lock(&path, Duration::ZERO).unwrap();
        assert!(path.is_file());
        // Read after releasing: on Windows the lock is mandatory, so a second
        // handle can't read the file while it is held.
        drop(guard);
        assert_eq!(std::fs::read(&path).unwrap(), b"");
    }

    #[test]
    fn lock_keeps_an_existing_files_contents() {
        let dir = setup();
        let path = dir.path().join("pid.lock");
        std::fs::write(&path, "1234").unwrap();
        drop(lock(&path, Duration::ZERO).unwrap());
        // Opening for the lock must not truncate (checked after release, as
        // Windows blocks reads while the lock is held).
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1234");
    }

    #[test]
    fn exclusive_lock_works_on_a_read_only_file() {
        // An exclusive lock asks for a writable handle first (NFS needs one);
        // a file that can't be opened for writing still locks read-only.
        let dir = setup();
        let path = lock_file(&dir);
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&path, perms.clone()).unwrap();

        let guard = lock(&path, Duration::ZERO).unwrap();
        assert!(lock(&path, Duration::ZERO).is_err());
        drop(guard);

        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&path, perms).unwrap();
    }

    #[test]
    fn lock_error_names_path_when_parent_is_missing() {
        let dir = setup();
        let path = dir.path().join("missing").join("x.lock");
        let err = lock(&path, Duration::ZERO).unwrap_err().to_string();
        assert!(err.contains(&path.display().to_string()), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn lock_works_on_a_directory() {
        let dir = setup();
        let guard = lock(dir.path(), Duration::ZERO).unwrap();
        assert!(lock_shared(dir.path(), Duration::ZERO).is_err());
        drop(guard);
        let _shared = lock_shared(dir.path(), Duration::ZERO).unwrap();
    }

    /// Pins the Windows half of `lock`'s docs. std only calls the interaction
    /// with reads and writes "platform specific", so this catches it changing.
    #[cfg(windows)]
    #[test]
    fn windows_locks_are_mandatory_as_documented() {
        use std::io::Write;
        const ERROR_LOCK_VIOLATION: i32 = 33;

        let dir = setup();
        let path = dir.path().join("data.lock");
        std::fs::write(&path, "1234").unwrap();
        {
            let _shared = lock_shared(&path, Duration::ZERO).unwrap();
            // A shared lock lets other handles read, but not write.
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "1234");
            let mut other = OpenOptions::new().write(true).open(&path).unwrap();
            let err = other.write_all(b"x").unwrap_err();
            assert_eq!(err.raw_os_error(), Some(ERROR_LOCK_VIOLATION), "{err}");
        }
        {
            // An exclusive lock stops other handles reading too.
            let _exclusive = lock(&path, Duration::ZERO).unwrap();
            let err = std::fs::read_to_string(&path).unwrap_err();
            assert_eq!(err.raw_os_error(), Some(ERROR_LOCK_VIOLATION), "{err}");
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1234");
    }

    #[cfg(windows)]
    #[test]
    fn lock_on_a_directory_is_a_clear_error_on_windows() {
        let dir = setup();
        let err = lock(dir.path(), Duration::ZERO).unwrap_err().to_string();
        assert!(err.contains("Windows can't lock directories"), "{err}");
        assert!(
            err.contains("lock a dedicated lock file next to it"),
            "{err}"
        );
    }

    /// The point of `flock` semantics: a shell script using `flock(1)` and a
    /// smop script exclude each other. Linux only: macOS has no `flock(1)`.
    #[cfg(target_os = "linux")]
    #[test]
    fn exclusive_lock_blocks_flock_command_until_dropped() {
        use std::process::{Command, Stdio};

        if Command::new("flock").arg("--version").output().is_err() {
            eprintln!("skipping: flock(1) is not installed");
            return;
        }
        let dir = setup();
        let guard = lock(dir.path(), Duration::ZERO).unwrap();

        // -w 10 bounds the wait, so a bug can't hang the test run.
        let mut child = Command::new("flock")
            .args(["-s", "-w", "10"])
            .arg(dir.path())
            .arg("true")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            child.try_wait().unwrap().is_none(),
            "flock -s should wait while the exclusive lock is held"
        );

        drop(guard);
        let start = Instant::now();
        let status = child.wait().unwrap();
        assert!(status.success(), "flock -s should get the lock: {status}");
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    // ------------------------------------------------------------------
    // copy, copy_dir, move_path, remove_if_exists, bytes, walk
    // ------------------------------------------------------------------

    /// Lists a tree as sorted lines: `d rel`, `f rel=content`, `l rel->target`.
    /// Uses std only, so it does not depend on the code under test.
    fn snapshot(root: &Path) -> Vec<String> {
        fn visit(root: &Path, dir: &Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let meta = std::fs::symlink_metadata(&path).unwrap();
                if meta.file_type().is_symlink() {
                    let target = std::fs::read_link(&path).unwrap();
                    out.push(format!("l {rel}->{}", target.display()));
                } else if meta.is_dir() {
                    out.push(format!("d {rel}"));
                    visit(root, &path, out);
                } else {
                    let content = std::fs::read_to_string(&path).unwrap();
                    out.push(format!("f {rel}={content}"));
                }
            }
        }
        let mut out = Vec::new();
        visit(root, root, &mut out);
        out.sort();
        out
    }

    /// Makes `src/{a.txt, empty/, nested/deep/b.txt}` in `dir`.
    fn sample_tree(dir: &TempDir) -> PathBuf {
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("empty")).unwrap();
        std::fs::create_dir_all(src.join("nested/deep")).unwrap();
        std::fs::write(src.join("a.txt"), "A").unwrap();
        std::fs::write(src.join("nested/deep/b.txt"), "B").unwrap();
        src
    }

    /// True when the process ignores directory permissions (root, or
    /// CAP_DAC_OVERRIDE). Then permission tests can't fail as they should.
    #[cfg(unix)]
    fn ignores_permissions(dir: &TempDir) -> bool {
        use std::os::unix::fs::PermissionsExt;
        let probe = dir.path().join("probe");
        std::fs::create_dir(&probe).unwrap();
        std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o000)).unwrap();
        let ignored = std::fs::read_dir(&probe).is_ok();
        std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o755)).unwrap();
        ignored
    }

    #[test]
    fn copy_on_a_directory_errors_naming_copy_dir() {
        let dir = setup();
        let src = sample_tree(&dir);
        let dst = dir.path().join("dst");

        let err = format!("{:#}", copy(&src, &dst).unwrap_err());
        assert!(err.contains("fs::copy_dir"), "{err}");
        assert!(err.contains(&src.display().to_string()), "{err}");
        assert!(!dst.exists());
    }

    #[test]
    fn copy_dir_reproduces_nested_and_empty_directories() {
        let dir = setup();
        let src = sample_tree(&dir);
        let dst = dir.path().join("dst");

        copy_dir(&src, &dst).unwrap();

        assert_eq!(
            snapshot(&dst),
            [
                "d empty",
                "d nested",
                "d nested/deep",
                "f a.txt=A",
                "f nested/deep/b.txt=B"
            ]
        );
        assert_eq!(snapshot(&src), snapshot(&dst));
    }

    #[test]
    fn copy_dir_errors_when_destination_exists_and_leaves_it_unchanged() {
        let dir = setup();
        let src = sample_tree(&dir);
        let dst = dir.path().join("dst");
        std::fs::create_dir(&dst).unwrap();
        std::fs::write(dst.join("keep.txt"), "K").unwrap();

        let err = format!("{:#}", copy_dir(&src, &dst).unwrap_err());
        assert!(err.contains("already exists"), "{err}");
        assert!(err.contains(&dst.display().to_string()), "{err}");
        assert_eq!(snapshot(&dst), ["f keep.txt=K"]);
    }

    #[test]
    fn copy_dir_on_a_file_errors_naming_copy() {
        let dir = setup();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();

        let err = format!("{:#}", copy_dir(&file, dir.path().join("dst")).unwrap_err());
        assert!(err.contains("fs::copy"), "{err}");
        assert!(err.contains(&file.display().to_string()), "{err}");
    }

    #[test]
    fn copy_dir_errors_naming_a_missing_source() {
        let dir = setup();
        let missing = dir.path().join("missing");
        let err = format!(
            "{:#}",
            copy_dir(&missing, dir.path().join("dst")).unwrap_err()
        );
        assert!(err.contains(&missing.display().to_string()), "{err}");
    }

    #[test]
    fn copy_dir_into_its_own_subdirectory_errors() {
        let dir = setup();
        let src = sample_tree(&dir);
        let inside = src.join("nested/copy");

        let err = format!("{:#}", copy_dir(&src, &inside).unwrap_err());
        assert!(err.contains("inside"), "{err}");
        assert!(!inside.exists());
    }

    #[cfg(unix)]
    #[test]
    fn copy_dir_copies_symlinks_as_links() {
        use std::os::unix::fs::symlink;
        let dir = setup();
        let src = sample_tree(&dir);
        symlink("a.txt", src.join("to_file")).unwrap();
        symlink("nested", src.join("to_dir")).unwrap();
        symlink("missing", src.join("dangling")).unwrap();
        // A cycle: copying it as a link must not recurse.
        symlink("..", src.join("nested/up")).unwrap();
        let dst = dir.path().join("dst");

        copy_dir(&src, &dst).unwrap();

        assert_eq!(snapshot(&src), snapshot(&dst));
        assert!(
            std::fs::symlink_metadata(dst.join("to_dir"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[cfg(unix)]
    #[test]
    fn copy_dir_keeps_directory_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = setup();
        let src = sample_tree(&dir);
        std::fs::set_permissions(src.join("nested"), std::fs::Permissions::from_mode(0o700))
            .unwrap();
        let dst = dir.path().join("dst");

        copy_dir(&src, &dst).unwrap();

        let mode = std::fs::metadata(dst.join("nested"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    /// A socket is neither a file, a directory nor a link, so copying it
    /// fails. This works as root too, unlike a permission error.
    #[cfg(unix)]
    #[test]
    fn copy_dir_removes_a_partial_copy_on_failure() {
        let dir = setup();
        let src = sample_tree(&dir);
        let socket = src.join("nested/zz.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let dst = dir.path().join("dst");

        let err = format!("{:#}", copy_dir(&src, &dst).unwrap_err());
        assert!(err.contains(&socket.display().to_string()), "{err}");
        assert!(!dst.exists(), "partial copy left behind");
        assert!(src.join("a.txt").exists());
    }

    #[test]
    fn move_path_moves_a_file_within_one_file_system() {
        let dir = setup();
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        std::fs::write(&from, "data").unwrap();

        move_path(&from, &to).unwrap();

        assert!(!from.exists());
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "data");
    }

    #[test]
    fn move_path_moves_a_directory_within_one_file_system() {
        let dir = setup();
        let src = sample_tree(&dir);
        let before = snapshot(&src);
        let to = dir.path().join("moved");

        move_path(&src, &to).unwrap();

        assert!(!src.exists());
        assert_eq!(snapshot(&to), before);
    }

    #[test]
    fn move_path_replaces_an_existing_file() {
        let dir = setup();
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        std::fs::write(&from, "new").unwrap();
        std::fs::write(&to, "old").unwrap();

        move_path(&from, &to).unwrap();

        assert_eq!(std::fs::read_to_string(&to).unwrap(), "new");
    }

    #[test]
    fn move_path_errors_when_a_directory_destination_exists() {
        let dir = setup();
        let src = sample_tree(&dir);
        let to = dir.path().join("taken");
        std::fs::create_dir(&to).unwrap();

        let err = format!("{:#}", move_path(&src, &to).unwrap_err());
        assert!(err.contains("already exists"), "{err}");
        assert!(src.join("a.txt").exists());
    }

    #[test]
    fn move_path_errors_naming_a_missing_source() {
        let dir = setup();
        let missing = dir.path().join("missing");
        let err = format!(
            "{:#}",
            move_path(&missing, dir.path().join("to")).unwrap_err()
        );
        assert!(err.contains(&missing.display().to_string()), "{err}");
    }

    fn cross_device(_: &Path, _: &Path) -> std::io::Result<()> {
        Err(std::io::ErrorKind::CrossesDevices.into())
    }

    #[test]
    fn move_path_copies_and_removes_a_file_on_a_cross_device_error() {
        let dir = setup();
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        std::fs::write(&from, "data").unwrap();

        move_path_with(&from, &to, cross_device).unwrap();

        assert!(!from.exists());
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "data");
    }

    #[test]
    fn move_path_copies_and_removes_a_directory_on_a_cross_device_error() {
        let dir = setup();
        let src = sample_tree(&dir);
        let before = snapshot(&src);
        let to = dir.path().join("moved");

        move_path_with(&src, &to, cross_device).unwrap();

        assert!(!src.exists());
        assert_eq!(snapshot(&to), before);
    }

    #[test]
    fn move_path_does_not_fall_back_on_other_errors() {
        let dir = setup();
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        std::fs::write(&from, "data").unwrap();

        let denied = |_: &Path, _: &Path| Err(std::io::ErrorKind::PermissionDenied.into());
        let err = format!("{:#}", move_path_with(&from, &to, denied).unwrap_err());

        assert!(err.contains(&from.display().to_string()), "{err}");
        assert!(from.exists());
        assert!(!to.exists());
    }

    #[cfg(unix)]
    #[test]
    fn move_path_removes_a_partial_copy_and_keeps_the_source_on_failure() {
        let dir = setup();
        let src = sample_tree(&dir);
        let _listener = std::os::unix::net::UnixListener::bind(src.join("zz.sock")).unwrap();
        let before = snapshot_without_sockets(&src);
        let to = dir.path().join("moved");

        assert!(move_path_with(&src, &to, cross_device).is_err());

        assert!(!to.exists(), "partial copy left behind");
        assert_eq!(snapshot_without_sockets(&src), before);
    }

    #[cfg(unix)]
    fn snapshot_without_sockets(root: &Path) -> Vec<String> {
        use std::os::unix::fs::FileTypeExt;
        let sock = root.join("zz.sock");
        assert!(
            std::fs::symlink_metadata(&sock)
                .unwrap()
                .file_type()
                .is_socket()
        );
        let mut lines = Vec::new();
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path != sock {
                lines.push(path.display().to_string());
            }
        }
        lines.sort();
        lines.extend(snapshot(&root.join("nested")));
        lines
    }

    /// Moves between two real file systems (for example tmpfs and disk).
    /// Skipped when no two candidate directories are on different devices.
    #[cfg(unix)]
    #[test]
    fn move_path_works_across_two_file_systems() {
        use std::os::unix::fs::MetadataExt;
        let candidates = [
            std::env::temp_dir(),
            PathBuf::from("/dev/shm"),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"),
        ];
        let dirs: Vec<TempDir> = candidates
            .iter()
            .filter_map(|c| {
                tempfile::Builder::new()
                    .prefix("smop-mv")
                    .tempdir_in(c)
                    .ok()
            })
            .collect();
        let dev = |d: &TempDir| std::fs::metadata(d.path()).unwrap().dev();
        let Some((a, b)) = dirs
            .iter()
            .enumerate()
            .flat_map(|(i, a)| dirs[i + 1..].iter().map(move |b| (a, b)))
            .find(|(a, b)| dev(a) != dev(b))
        else {
            eprintln!("skipping: no two file systems among {candidates:?}");
            return;
        };

        let src = a.path().join("src");
        std::fs::create_dir_all(src.join("empty")).unwrap();
        std::fs::write(src.join("f.txt"), "F").unwrap();
        let before = snapshot(&src);
        let to = b.path().join("moved");
        // Check the fallback is what runs here.
        let rename_err = std::fs::rename(&src, &to).unwrap_err();
        assert_eq!(rename_err.kind(), std::io::ErrorKind::CrossesDevices);

        move_path(&src, &to).unwrap();
        assert!(!src.exists());
        assert_eq!(snapshot(&to), before);

        let file = b.path().join("moved/f.txt");
        let back = a.path().join("back.txt");
        move_path(&file, &back).unwrap();
        assert!(!file.exists());
        assert_eq!(std::fs::read_to_string(&back).unwrap(), "F");
    }

    #[test]
    fn remove_if_exists_is_ok_on_a_missing_path() {
        let dir = setup();
        remove_if_exists(dir.path().join("missing")).unwrap();
    }

    #[test]
    fn remove_if_exists_removes_files_and_directories() {
        let dir = setup();
        let src = sample_tree(&dir);
        let file = src.join("a.txt");

        remove_if_exists(&file).unwrap();
        assert!(!file.exists());
        remove_if_exists(&src).unwrap();
        assert!(!src.exists());
    }

    /// Unix only: Windows reports a path through a file as "not found".
    #[cfg(unix)]
    #[test]
    fn remove_if_exists_errors_on_a_path_through_a_file() {
        let dir = setup();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();
        let bad = file.join("child");

        let err = format!("{:#}", remove_if_exists(&bad).unwrap_err());
        assert!(err.contains(&bad.display().to_string()), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn remove_if_exists_removes_a_directory_symlink_but_not_its_target() {
        let dir = setup();
        let src = sample_tree(&dir);
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&src, &link).unwrap();

        remove_if_exists(&link).unwrap();

        assert!(std::fs::symlink_metadata(&link).is_err());
        assert!(src.join("a.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn remove_if_exists_still_errors_on_permission_denied() {
        use std::os::unix::fs::PermissionsExt;
        let dir = setup();
        if ignores_permissions(&dir) {
            eprintln!("skipping: this process ignores file permissions (root?)");
            return;
        }
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        let file = locked.join("f.txt");
        std::fs::write(&file, "x").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();

        let result = remove_if_exists(&file);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

        let err = format!("{:#}", result.unwrap_err());
        assert!(err.contains(&file.display().to_string()), "{err}");
        assert!(file.exists());
    }

    #[test]
    fn read_bytes_and_write_bytes_round_trip_non_utf8() {
        let dir = setup();
        let path = dir.path().join("blob.bin");
        let bytes = [0xff, 0xfe, 0x00, 0xc3, 0x28, b'\n', 0x80];
        assert!(String::from_utf8(bytes.to_vec()).is_err());

        write_bytes(&path, bytes).unwrap();

        assert_eq!(read_bytes(&path).unwrap(), bytes);
    }

    #[test]
    fn read_bytes_error_names_the_path() {
        let dir = setup();
        let path = dir.path().join("missing.bin");
        let err = format!("{:#}", read_bytes(&path).unwrap_err());
        assert!(err.contains(&path.display().to_string()), "{err}");
    }

    #[test]
    fn write_bytes_error_names_the_path() {
        let dir = setup();
        let path = dir.path().join("missing").join("blob.bin");
        let err = format!("{:#}", write_bytes(&path, b"x").unwrap_err());
        assert!(err.contains(&path.display().to_string()), "{err}");
    }

    #[test]
    fn walk_returns_files_only_sorted_by_path() {
        let dir = setup();
        let src = sample_tree(&dir);
        std::fs::write(src.join("z.txt"), "").unwrap();
        std::fs::write(src.join("nested/c.txt"), "").unwrap();

        let files = walk(&src).unwrap();

        assert_eq!(
            files,
            [
                src.join("a.txt"),
                src.join("nested/c.txt"),
                src.join("nested/deep/b.txt"),
                src.join("z.txt"),
            ]
        );
    }

    #[test]
    fn walk_of_an_empty_directory_is_empty() {
        let dir = setup();
        assert!(walk(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn walk_errors_naming_a_file_or_missing_path() {
        let dir = setup();
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "x").unwrap();
        let missing = dir.path().join("missing");

        for path in [file, missing] {
            let err = format!("{:#}", walk(&path).unwrap_err());
            assert!(err.contains(&path.display().to_string()), "{err}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn walk_does_not_follow_directory_symlinks_and_ends_on_a_cycle() {
        use std::os::unix::fs::symlink;
        let dir = setup();
        let src = sample_tree(&dir);
        symlink("..", src.join("nested/up")).unwrap();
        symlink(".", src.join("self")).unwrap();
        symlink("a.txt", src.join("to_file")).unwrap();

        let files = walk(&src).unwrap();

        assert_eq!(
            files,
            [
                src.join("a.txt"),
                src.join("nested/deep/b.txt"),
                src.join("to_file")
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn walk_errors_naming_an_unreadable_directory() {
        use std::os::unix::fs::PermissionsExt;
        let dir = setup();
        if ignores_permissions(&dir) {
            eprintln!("skipping: this process ignores file permissions (root?)");
            return;
        }
        let src = sample_tree(&dir);
        let locked = src.join("nested");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

        let result = walk(&src);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

        let err = format!("{:#}", result.unwrap_err());
        assert!(err.contains(&locked.display().to_string()), "{err}");
    }
}
