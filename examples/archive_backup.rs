//! Backs up a directory to a timestamped `.tar.gz`, restores it to a second
//! directory, and makes sure that the two trees are the same. Writes a JSON
//! manifest.
//!
//! ```sh
//! cargo run --example archive_backup
//! ```
//!
//! The example makes its own sample files in
//! `target/smop-example-output/archive-backup`.

use std::path::{Path, PathBuf};

use smop::prelude::*;

/// What the backup contains.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(crate = "smop::serde")]
struct Manifest {
    created_at: String,
    archive: PathBuf,
    files: Vec<String>,
}

fn main() -> Result<()> {
    let out = Path::new("target/smop-example-output/archive-backup");
    fs::remove_if_exists(out)?;

    let source = out.join("source");
    for (name, text) in [
        ("README.txt", "Back up this directory with smop\n"),
        ("config/app.toml", "port = 8080\nmode = \"demo\"\n"),
        ("notes/todo.txt", "- rotate the logs\n"),
    ] {
        let file = source.join(name);
        fs::create_dir_all(file.parent().context("no parent directory")?)?;
        fs::write_string(&file, text)?;
    }

    let manifest = backup(&source, out, &time::now())?;
    let manifest_path = out.join("manifest.json");
    fs::write_json(&manifest_path, &manifest)?;

    println!("Archive: {}", manifest.archive.display());
    println!("Restored and checked {} files", manifest.files.len());
    println!("Manifest: {}", manifest_path.display());
    Ok(())
}

/// Archives `source` into `out`, extracts it to `out/restored`, and compares
/// each file.
///
/// # Errors
///
/// Returns an error if a step fails, or if the restored tree is not the same
/// as `source`.
fn backup(source: &Path, out: &Path, created_at: &time::DateTime<time::Utc>) -> Result<Manifest> {
    let restored = out.join("restored");
    ensure!(!restored.exists(), "{}: it exists", restored.display());

    let archive = out.join(format!("{}.tar.gz", snapshot_name(created_at)));
    archive::create_tar_gz(source, &archive)?;
    archive::extract(&archive, &restored)?;

    let files = relative_files(source)?;
    let restored_files = relative_files(&restored)?;
    ensure!(
        files == restored_files,
        "{}: the files are not the same as in {}",
        restored.display(),
        source.display()
    );
    for file in &files {
        let (a, b) = (source.join(file), restored.join(file));
        ensure!(
            fs::read_bytes(&a)? == fs::read_bytes(&b)?,
            "{}: the content is not the same",
            b.display()
        );
    }

    Ok(Manifest {
        created_at: time::format(created_at, "%Y-%m-%dT%H:%M:%SZ"),
        archive,
        files,
    })
}

/// The archive name for a time: `smop-backup-YYYYMMDD-HHMMSS`.
fn snapshot_name(created_at: &time::DateTime<time::Utc>) -> String {
    format!("smop-backup-{}", time::format(created_at, "%Y%m%d-%H%M%S"))
}

/// The files below `dir`, relative to `dir`, sorted, with `/` separators.
fn relative_files(dir: &Path) -> Result<Vec<String>> {
    let mut files = fs::walk(dir)?
        .iter()
        .map(|file| {
            let rel = file.strip_prefix(dir)?;
            Ok(rel.to_string_lossy().replace('\\', "/"))
        })
        .collect::<Result<Vec<_>>>()?;
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> time::DateTime<time::Utc> {
        time::parse(text, "%Y-%m-%d %H:%M:%S").unwrap().and_utc()
    }

    fn sample(dir: &Path) {
        fs::create_dir_all(dir.join("notes")).unwrap();
        fs::write_string(dir.join("README.txt"), "root").unwrap();
        fs::write_string(dir.join("notes/todo.txt"), "nested").unwrap();
    }

    #[test]
    fn snapshot_name_uses_a_compact_timestamp() {
        let name = snapshot_name(&at("2026-11-03 20:00:15"));
        assert_eq!(name, "smop-backup-20261103-200015");
    }

    #[test]
    fn relative_files_are_sorted_with_forward_slashes() {
        let temp = fs::temp_dir().unwrap();
        sample(temp.path());
        let files = relative_files(temp.path()).unwrap();
        assert_eq!(files, ["README.txt", "notes/todo.txt"]);
    }

    #[test]
    fn relative_files_of_a_missing_directory_is_an_error() {
        let temp = fs::temp_dir().unwrap();
        assert!(relative_files(&temp.path().join("missing")).is_err());
    }

    #[test]
    fn backup_restores_each_file() {
        let temp = fs::temp_dir().unwrap();
        let source = temp.path().join("source");
        sample(&source);

        let manifest = backup(&source, temp.path(), &at("2026-11-03 20:00:15")).unwrap();

        assert_eq!(manifest.created_at, "2026-11-03T20:00:15Z");
        assert_eq!(manifest.files, ["README.txt", "notes/todo.txt"]);
        assert!(
            manifest
                .archive
                .ends_with("smop-backup-20261103-200015.tar.gz")
        );
        assert!(manifest.archive.is_file());
        let restored = fs::read_string(temp.path().join("restored/notes/todo.txt")).unwrap();
        assert_eq!(restored, "nested");
    }

    #[test]
    fn backup_into_a_used_directory_is_an_error() {
        let temp = fs::temp_dir().unwrap();
        let source = temp.path().join("source");
        sample(&source);
        fs::create_dir_all(temp.path().join("restored")).unwrap();
        fs::write_string(temp.path().join("restored/extra.txt"), "old").unwrap();

        let err = backup(&source, temp.path(), &at("2026-11-03 20:00:15")).unwrap_err();
        assert!(format!("{err:#}").contains("it exists"), "{err:#}");
        let files = fs::walk(temp.path()).unwrap();
        assert!(
            !files
                .iter()
                .any(|f| f.to_string_lossy().ends_with(".tar.gz")),
            "no archive is left: {files:?}"
        );
    }
}
