//! Archive utilities (Zip).
//!
//! Provides functions to create and extract zip archives.
//! This module is part of the `archive` feature.

use std::fs::{self, File};
use std::io::{Read, Write, copy};
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use walkdir::WalkDir;
use zip::write::FileOptions;

/// Creates a zip archive from a directory.
///
/// The content of the source directory will be at the root of the zip file.
///
/// # Errors
///
/// Returns an error if the source directory doesn't exist or if file operations fail.
///
/// # Examples
///
/// ```no_run
/// use smop::experimental::archive;
///
/// archive::zip_dir("src_dir", "archive.zip")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn zip_dir<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    if !src.is_dir() {
        return Err(anyhow!("Source path is not a directory: {}", src.display()));
    }

    let file = File::create(dst)
        .with_context(|| format!("Failed to create zip file: {}", dst.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    let walk = WalkDir::new(src);
    let mut buffer = Vec::new();

    for entry in walk {
        let entry =
            entry.with_context(|| format!("Failed to walk directory: {}", src.display()))?;
        let path = entry.path();

        // Calculate relative path for the zip entry
        let name = path
            .strip_prefix(src)
            .map_err(|e| anyhow!("Failed to strip prefix: {e}"))?
            .to_str()
            .ok_or_else(|| anyhow!("Path is not valid UTF-8: {}", path.display()))?;

        // Skip the root folder itself (empty name)
        if name.is_empty() {
            continue;
        }

        // Normalize path separators to forward slashes for zip compatibility
        let name = name.replace('\\', "/");

        if path.is_file() {
            zip.start_file(&name, options)
                .with_context(|| format!("Failed to start file in zip: {name}"))?;

            let mut f = File::open(path)
                .with_context(|| format!("Failed to open file: {}", path.display()))?;
            f.read_to_end(&mut buffer)
                .with_context(|| format!("Failed to read file: {}", path.display()))?;
            zip.write_all(&buffer)
                .with_context(|| format!("Failed to write file to zip: {name}"))?;
            buffer.clear();
        } else if !name.is_empty() {
            // It's a directory
            zip.add_directory(&name, options)
                .with_context(|| format!("Failed to add directory to zip: {name}"))?;
        }
    }

    zip.finish()
        .with_context(|| format!("Failed to finish zip file: {}", dst.display()))?;

    Ok(())
}

/// Extracts a zip archive to a directory.
///
/// # Errors
///
/// Returns an error if the zip file doesn't exist or is invalid.
///
/// # Examples
///
/// ```no_run
/// use smop::experimental::archive;
///
/// archive::unzip("archive.zip", "dst_dir")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn unzip<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> Result<()> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    let file =
        File::open(src).with_context(|| format!("Failed to open zip file: {}", src.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("Failed to read zip archive: {}", src.display()))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .with_context(|| "Failed to read zip entry")?;
        let outpath = match file.enclosed_name() {
            Some(path) => dst.join(path),
            None => continue, // Skip suspicious paths (Zip Slip)
        };

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)
                .with_context(|| format!("Failed to create directory: {}", outpath.display()))?;
        } else {
            if let Some(p) = outpath.parent() {
                fs::create_dir_all(p).with_context(|| {
                    format!("Failed to create parent directory: {}", p.display())
                })?;
            }
            let mut outfile = File::create(&outpath).with_context(|| {
                format!("Failed to create extracted file: {}", outpath.display())
            })?;
            copy(&mut file, &mut outfile)
                .with_context(|| format!("Failed to extract file: {}", outpath.display()))?;
        }

        // Get and set permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&outpath, fs::Permissions::from_mode(mode)).with_context(
                    || format!("Failed to set permissions for: {}", outpath.display()),
                )?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn test_zip_and_unzip() {
        let dir = setup();
        let src_dir = dir.path().join("src");
        fs::create_dir(&src_dir).unwrap();

        fs::write(src_dir.join("file1.txt"), "Hello").unwrap();
        fs::create_dir(src_dir.join("subdir")).unwrap();
        fs::write(src_dir.join("subdir/file2.txt"), "World").unwrap();

        let zip_path = dir.path().join("archive.zip");
        zip_dir(&src_dir, &zip_path).unwrap();

        assert!(zip_path.exists());

        let dst_dir = dir.path().join("dst");
        unzip(&zip_path, &dst_dir).unwrap();

        assert!(dst_dir.join("file1.txt").exists());
        assert_eq!(
            fs::read_to_string(dst_dir.join("file1.txt")).unwrap(),
            "Hello"
        );
        assert!(dst_dir.join("subdir/file2.txt").exists());
        assert_eq!(
            fs::read_to_string(dst_dir.join("subdir/file2.txt")).unwrap(),
            "World"
        );
    }

    #[test]
    fn test_zip_dir_fails_on_non_dir() {
        let dir = setup();
        let file_path = dir.path().join("file.txt");
        fs::write(&file_path, "test").unwrap();

        let result = zip_dir(&file_path, dir.path().join("out.zip"));
        assert!(result.is_err());
    }

    #[test]
    fn test_unzip_fails_on_missing_file() {
        let dir = setup();
        let result = unzip(dir.path().join("missing.zip"), dir.path().join("out"));
        assert!(result.is_err());
    }
}
