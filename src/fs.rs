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

/// Copies a file or directory from one location to another.
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
/// fs::copy("source.txt", "dest.txt")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
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
// Advisory locks
// ============================================================================

/// How often [`lock`] and [`lock_shared`] retry while another holder has the
/// lock. std has no blocking lock with a timeout, so waiting means polling.
const LOCK_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

/// An advisory lock on a file or directory, from [`lock`] or [`lock_shared`].
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
/// This is the lock `flock -x` takes on Unix, so it works together with
/// `flock(1)` and other programs using `flock(2)` on the same path. On Windows
/// it is a `LockFileEx` lock.
///
/// - `path` may be a file or, on Unix, a directory. Like `flock(1)`, a missing
///   path is created as an empty file (its parent directory must exist).
/// - While another process (or another handle in this one) holds the lock,
///   this retries every 50 ms until `timeout` passes. `Duration::ZERO` tries
///   once; `Duration::MAX` waits indefinitely.
/// - The lock is released when the returned [`FileLock`] is dropped, or when
///   the process exits.
///
/// On Unix the lock is advisory: it only excludes other code that also locks
/// the path. On Windows it is mandatory: while an exclusive lock is held,
/// other handles can neither read nor write the file (a shared lock still lets
/// them read). Lock a dedicated lock file, as below, rather than a file you
/// also read or write, and the code behaves the same on both.
///
/// # Errors
///
/// Returns an error naming the path if it can't be opened or locked, or if
/// another holder still has the lock when `timeout` runs out. On Windows,
/// locking a directory is an error: lock a file inside it instead.
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
/// waits for all of them. This is the lock `flock -s` takes on Unix. Paths,
/// timeouts and release work as in [`lock`].
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
             lock a file inside it instead",
            path.display()
        );
    }

    let file = open_for_lock(path)
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
/// (as `flock(1)` does). A read-only handle is enough for `flock(2)` and for
/// `LockFileEx`, and on Unix it also opens directories.
fn open_for_lock(path: &Path) -> std::io::Result<fs::File> {
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
    // Advisory locks
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
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            drop(held);
        });

        let start = Instant::now();
        let guard = lock(&path, Duration::from_secs(5)).unwrap();
        assert!(start.elapsed() >= Duration::from_millis(100));
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

    #[cfg(windows)]
    #[test]
    fn lock_on_a_directory_is_a_clear_error_on_windows() {
        let dir = setup();
        let err = lock(dir.path(), Duration::ZERO).unwrap_err().to_string();
        assert!(err.contains("Windows can't lock directories"), "{err}");
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
}
