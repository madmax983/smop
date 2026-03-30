//! Example: Create and restore a timestamped archive snapshot.
//!
//! Run with: `cargo run --example archive_backup`

use smop::prelude::*;
use std::fs as stdfs;
use std::path::Path;

#[derive(Debug, Serialize, PartialEq, Eq)]
struct SnapshotManifest {
    snapshot_name: String,
    created_at: String,
    archive_path: String,
    source_files: Vec<String>,
    restored_files: Vec<String>,
}

fn main() -> Result<()> {
    let output_dir = path::expand("./target/smop-example-output/archive-backup");
    if output_dir.exists() {
        fs::remove(&output_dir)?;
    }

    let source_dir = output_dir.join("source");
    let restored_dir = output_dir.join("restored");
    stdfs::create_dir_all(source_dir.join("config")).with_context(|| {
        format!(
            "Failed to create source directory: {}",
            source_dir.display()
        )
    })?;
    stdfs::create_dir_all(source_dir.join("notes")).with_context(|| {
        format!(
            "Failed to create source directory: {}",
            source_dir.display()
        )
    })?;

    fs::write_string(
        source_dir.join("README.txt"),
        "Archive this directory with smop\n",
    )?;
    fs::write_string(
        source_dir.join("config/app.toml"),
        "port = 8080\nmode = \"demo\"\n",
    )?;
    fs::write_string(
        source_dir.join("notes/todo.txt"),
        "- backup logs\n- rotate credentials\n",
    )?;

    let created_at = time::now();
    let snapshot_name = snapshot_name(&created_at);
    let archive_path = output_dir.join(format!("{snapshot_name}.tar.gz"));

    archive::create_tar_gz(&source_dir, &archive_path)?;
    stdfs::create_dir_all(&restored_dir).with_context(|| {
        format!(
            "Failed to create restore directory: {}",
            restored_dir.display()
        )
    })?;
    archive::extract_tar_gz(&archive_path, &restored_dir)?;

    let manifest = build_manifest(
        &created_at,
        &snapshot_name,
        &archive_path,
        &source_dir,
        &restored_dir,
    )?;
    let manifest_path = output_dir.join("manifest.json");
    fs::write_json(&manifest_path, &manifest)?;

    println!("Created: {}", archive_path.display());
    println!("Restored files: {}", manifest.restored_files.len());
    println!("Manifest: {}", manifest_path.display());

    Ok(())
}

fn snapshot_name(created_at: &time::DateTime<time::Utc>) -> String {
    format!("smop-backup-{}", time::format(created_at, "%Y%m%d-%H%M%S"))
}

fn build_manifest(
    created_at: &time::DateTime<time::Utc>,
    snapshot_name: &str,
    archive_path: &Path,
    source_dir: &Path,
    restored_dir: &Path,
) -> Result<SnapshotManifest> {
    Ok(SnapshotManifest {
        snapshot_name: snapshot_name.to_string(),
        created_at: time::format(created_at, "%Y-%m-%dT%H:%M:%SZ"),
        archive_path: archive_path.display().to_string(),
        source_files: collect_relative_files(source_dir)?,
        restored_files: collect_relative_files(restored_dir)?,
    })
}

fn collect_relative_files(dir: &Path) -> Result<Vec<String>> {
    let normalized_root = stdfs::canonicalize(dir)
        .with_context(|| format!("Failed to canonicalize directory: {}", dir.display()))?;
    let pattern = format!("{}/**/*", dir.display().to_string().replace('\\', "/"));
    let mut files = fs::glob(&pattern)?
        .into_iter()
        .filter(|path| path.is_file())
        .map(|path| -> Result<String> {
            let normalized_path = stdfs::canonicalize(&path)
                .with_context(|| format!("Failed to canonicalize file: {}", path.display()))?;
            let relative = normalized_path
                .strip_prefix(&normalized_root)
                .with_context(|| format!("Failed to strip prefix: {}", path.display()))?;
            Ok(relative.to_string_lossy().replace('\\', "/"))
        })
        .collect::<Result<Vec<_>>>()?;

    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn snapshot_name_uses_compact_timestamp() {
        let parsed = time::parse("2026-03-29 12:34:56", "%Y-%m-%d %H:%M:%S").unwrap();
        let created_at = time::DateTime::<time::Utc>::from_naive_utc_and_offset(parsed, time::Utc);

        assert_eq!(snapshot_name(&created_at), "smop-backup-20260329-123456");
    }

    #[test]
    fn collect_relative_files_returns_sorted_paths() {
        let temp = fs::temp_dir().unwrap();
        let nested = temp.path().join("notes");
        stdfs::create_dir_all(&nested).unwrap();
        fs::write_string(temp.path().join("README.txt"), "root").unwrap();
        fs::write_string(nested.join("todo.txt"), "nested").unwrap();

        let files = collect_relative_files(temp.path()).unwrap();

        assert_eq!(files, vec!["README.txt", "notes/todo.txt"]);
    }

    #[test]
    fn collect_relative_files_handles_dot_prefixed_relative_roots() {
        let base = PathBuf::from("./target/archive-relative-test");
        if base.exists() {
            fs::remove(&base).unwrap();
        }

        let nested = base.join("notes");
        stdfs::create_dir_all(&nested).unwrap();
        fs::write_string(base.join("README.txt"), "root").unwrap();
        fs::write_string(nested.join("todo.txt"), "nested").unwrap();

        let files = collect_relative_files(&base).unwrap();

        assert_eq!(files, vec!["README.txt", "notes/todo.txt"]);
        fs::remove(&base).unwrap();
    }
}
