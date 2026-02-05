//! File system utilities.
//!
//! Convenient functions for common file operations like reading,
//! writing, and JSON serialization.

use std::fs::{self, OpenOptions};
use std::io::Write as IoWrite;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};

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
    #![allow(clippy::unwrap_used)]
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

    #[test]
    fn read_json_deserializes_correctly() {
        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct Data {
            name: String,
            value: i32,
        }

        let dir = setup();
        let path = dir.path().join("data.json");
        std::fs::write(&path, r#"{"name": "test", "value": 42}"#).unwrap();

        let data: Data = read_json(&path).unwrap();
        assert_eq!(data.name, "test");
        assert_eq!(data.value, 42);
    }

    #[test]
    fn read_json_fails_on_invalid_json() {
        let dir = setup();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not valid json").unwrap();

        let result: Result<serde_json::Value> = read_json(&path);
        assert!(result.is_err());
    }

    #[test]
    fn write_json_serializes_with_pretty_print() {
        #[derive(serde::Serialize)]
        struct Data {
            name: String,
        }

        let dir = setup();
        let path = dir.path().join("out.json");

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
        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct Record {
            name: String,
            age: u32,
        }

        let dir = setup();
        let path = dir.path().join("data.csv");
        std::fs::write(&path, "name,age\nAlice,30\nBob,25").unwrap();

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
        let result: Result<Vec<serde_json::Value>> = read_csv("/nonexistent/path/file.csv");
        assert!(result.is_err());
    }

    #[cfg(feature = "csv")]
    #[test]
    fn write_csv_creates_file_with_headers() {
        #[derive(serde::Serialize)]
        struct Record {
            name: String,
            score: i32,
        }

        let dir = setup();
        let path = dir.path().join("output.csv");

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
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Record {
            id: u32,
            value: String,
        }

        let dir = setup();
        let path = dir.path().join("roundtrip.csv");

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
}
