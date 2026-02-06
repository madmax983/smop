//! Archive creation and extraction (ZIP, TAR, TAR.GZ).
//!
//! Functions for working with compressed archives.
//! This module is only available with the `archive` feature.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, anyhow};

/// Extracts a ZIP archive to the specified directory.
///
/// # Errors
///
/// Returns an error if the archive can't be read or extraction fails.
///
/// # Examples
///
/// ```no_run
/// use smop::archive;
///
/// archive::extract_zip("archive.zip", "output_dir")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn extract_zip<P: AsRef<Path>, Q: AsRef<Path>>(archive: P, dest: Q) -> Result<()> {
    let archive = archive.as_ref();
    let dest = dest.as_ref();

    let file = File::open(archive)
        .with_context(|| format!("Failed to open ZIP archive: {}", archive.display()))?;

    let mut zip = zip::ZipArchive::new(file)
        .with_context(|| format!("Failed to read ZIP archive: {}", archive.display()))?;

    for i in 0..zip.len() {
        let mut file = zip.by_index(i).with_context(|| {
            format!("Failed to read entry {} from ZIP: {}", i, archive.display())
        })?;

        let outpath = match file.enclosed_name() {
            Some(path) => dest.join(path),
            None => {
                // Skip files with invalid paths (potential path traversal)
                continue;
            }
        };

        if file.name().ends_with('/') {
            // Directory
            fs::create_dir_all(&outpath)
                .with_context(|| format!("Failed to create directory: {}", outpath.display()))?;
        } else {
            // File
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create parent directory: {}", parent.display())
                })?;
            }

            let mut outfile = File::create(&outpath)
                .with_context(|| format!("Failed to create file: {}", outpath.display()))?;

            std::io::copy(&mut file, &mut outfile)
                .with_context(|| format!("Failed to extract file: {}", outpath.display()))?;
        }
    }

    Ok(())
}

/// Creates a ZIP archive from a directory or file.
///
/// # Errors
///
/// Returns an error if the source can't be read or archive creation fails.
///
/// # Examples
///
/// ```no_run
/// use smop::archive;
///
/// archive::create_zip("source_dir", "archive.zip")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn create_zip<P: AsRef<Path>, Q: AsRef<Path>>(source: P, dest: Q) -> Result<()> {
    let source = source.as_ref();
    let dest = dest.as_ref();

    let file = File::create(dest)
        .with_context(|| format!("Failed to create ZIP archive: {}", dest.display()))?;

    let mut zip = zip::ZipWriter::new(file);

    if source.is_file() {
        // Single file
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow!("Invalid source file name"))?;

        zip.start_file(name, options)
            .context("Failed to add file to ZIP")?;

        let mut f = File::open(source)
            .with_context(|| format!("Failed to open source file: {}", source.display()))?;

        std::io::copy(&mut f, &mut zip)
            .with_context(|| format!("Failed to write file to ZIP: {}", source.display()))?;
    } else if source.is_dir() {
        // Directory - walk recursively
        add_dir_to_zip(&mut zip, source, source)?;
    } else {
        return Err(anyhow!("Source must be a file or directory"));
    }

    zip.finish()
        .with_context(|| format!("Failed to finalize ZIP archive: {}", dest.display()))?;

    Ok(())
}

/// Helper to recursively add directory contents to ZIP
fn add_dir_to_zip<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    source: &Path,
    base: &Path,
) -> Result<()> {
    let entries = fs::read_dir(source)
        .with_context(|| format!("Failed to read directory: {}", source.display()))?;

    for entry in entries {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        let relative = path
            .strip_prefix(base)
            .with_context(|| format!("Failed to compute relative path for: {}", path.display()))?;

        let name = relative
            .to_str()
            .ok_or_else(|| anyhow!("Invalid UTF-8 in path: {}", relative.display()))?;

        if path.is_file() {
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

            zip.start_file(name, options)
                .with_context(|| format!("Failed to add file to ZIP: {name}"))?;

            let mut f = File::open(&path)
                .with_context(|| format!("Failed to open file: {}", path.display()))?;

            std::io::copy(&mut f, zip)
                .with_context(|| format!("Failed to write file to ZIP: {}", path.display()))?;
        } else if path.is_dir() {
            add_dir_to_zip(zip, &path, base)?;
        }
    }

    Ok(())
}

/// Extracts a TAR archive to the specified directory.
///
/// # Errors
///
/// Returns an error if the archive can't be read or extraction fails.
///
/// # Examples
///
/// ```no_run
/// use smop::archive;
///
/// archive::extract_tar("archive.tar", "output_dir")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn extract_tar<P: AsRef<Path>, Q: AsRef<Path>>(archive: P, dest: Q) -> Result<()> {
    let archive = archive.as_ref();
    let dest = dest.as_ref();

    let file = File::open(archive)
        .with_context(|| format!("Failed to open TAR archive: {}", archive.display()))?;

    let mut tar = tar::Archive::new(file);

    tar.unpack(dest)
        .with_context(|| format!("Failed to extract TAR archive to: {}", dest.display()))?;

    Ok(())
}

/// Creates a TAR archive from a directory or file.
///
/// # Errors
///
/// Returns an error if the source can't be read or archive creation fails.
///
/// # Examples
///
/// ```no_run
/// use smop::archive;
///
/// archive::create_tar("source_dir", "archive.tar")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn create_tar<P: AsRef<Path>, Q: AsRef<Path>>(source: P, dest: Q) -> Result<()> {
    let source = source.as_ref();
    let dest = dest.as_ref();

    let file = File::create(dest)
        .with_context(|| format!("Failed to create TAR archive: {}", dest.display()))?;

    let mut tar = tar::Builder::new(file);

    if source.is_file() {
        let name = source
            .file_name()
            .ok_or_else(|| anyhow!("Invalid source file name"))?;

        tar.append_path_with_name(source, name)
            .with_context(|| format!("Failed to add file to TAR: {}", source.display()))?;
    } else if source.is_dir() {
        tar.append_dir_all(".", source)
            .with_context(|| format!("Failed to add directory to TAR: {}", source.display()))?;
    } else {
        return Err(anyhow!("Source must be a file or directory"));
    }

    tar.finish()
        .with_context(|| format!("Failed to finalize TAR archive: {}", dest.display()))?;

    Ok(())
}

/// Extracts a TAR.GZ (gzipped TAR) archive to the specified directory.
///
/// # Errors
///
/// Returns an error if the archive can't be read or extraction fails.
///
/// # Examples
///
/// ```no_run
/// use smop::archive;
///
/// archive::extract_tar_gz("archive.tar.gz", "output_dir")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn extract_tar_gz<P: AsRef<Path>, Q: AsRef<Path>>(archive: P, dest: Q) -> Result<()> {
    let archive = archive.as_ref();
    let dest = dest.as_ref();

    let file = File::open(archive)
        .with_context(|| format!("Failed to open TAR.GZ archive: {}", archive.display()))?;

    let decompressor = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decompressor);

    tar.unpack(dest)
        .with_context(|| format!("Failed to extract TAR.GZ archive to: {}", dest.display()))?;

    Ok(())
}

/// Creates a TAR.GZ (gzipped TAR) archive from a directory or file.
///
/// # Errors
///
/// Returns an error if the source can't be read or archive creation fails.
///
/// # Examples
///
/// ```no_run
/// use smop::archive;
///
/// archive::create_tar_gz("source_dir", "archive.tar.gz")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn create_tar_gz<P: AsRef<Path>, Q: AsRef<Path>>(source: P, dest: Q) -> Result<()> {
    let source = source.as_ref();
    let dest = dest.as_ref();

    let file = File::create(dest)
        .with_context(|| format!("Failed to create TAR.GZ archive: {}", dest.display()))?;

    let compressor = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut tar = tar::Builder::new(compressor);

    if source.is_file() {
        let name = source
            .file_name()
            .ok_or_else(|| anyhow!("Invalid source file name"))?;

        tar.append_path_with_name(source, name)
            .with_context(|| format!("Failed to add file to TAR.GZ: {}", source.display()))?;
    } else if source.is_dir() {
        tar.append_dir_all(".", source)
            .with_context(|| format!("Failed to add directory to TAR.GZ: {}", source.display()))?;
    } else {
        return Err(anyhow!("Source must be a file or directory"));
    }

    tar.finish()
        .with_context(|| format!("Failed to finalize TAR.GZ archive: {}", dest.display()))?;

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
    fn zip_roundtrip_single_file() {
        let dir = setup();
        let source = dir.path().join("test.txt");
        let archive = dir.path().join("archive.zip");
        let extract = dir.path().join("extracted");

        fs::write(&source, "test content").unwrap();

        create_zip(&source, &archive).unwrap();
        assert!(archive.exists());

        fs::create_dir(&extract).unwrap();
        extract_zip(&archive, &extract).unwrap();

        let extracted_file = extract.join("test.txt");
        assert!(extracted_file.exists());
        assert_eq!(fs::read_to_string(&extracted_file).unwrap(), "test content");
    }

    #[test]
    fn zip_roundtrip_directory() {
        let dir = setup();
        let source = dir.path().join("source");
        let archive = dir.path().join("archive.zip");
        let extract = dir.path().join("extracted");

        fs::create_dir(&source).unwrap();
        fs::write(source.join("file1.txt"), "content1").unwrap();
        fs::write(source.join("file2.txt"), "content2").unwrap();

        create_zip(&source, &archive).unwrap();
        assert!(archive.exists());

        fs::create_dir(&extract).unwrap();
        extract_zip(&archive, &extract).unwrap();

        assert_eq!(
            fs::read_to_string(extract.join("file1.txt")).unwrap(),
            "content1"
        );
        assert_eq!(
            fs::read_to_string(extract.join("file2.txt")).unwrap(),
            "content2"
        );
    }

    #[test]
    fn tar_roundtrip_single_file() {
        let dir = setup();
        let source = dir.path().join("test.txt");
        let archive = dir.path().join("archive.tar");
        let extract = dir.path().join("extracted");

        fs::write(&source, "test content").unwrap();

        create_tar(&source, &archive).unwrap();
        assert!(archive.exists());

        fs::create_dir(&extract).unwrap();
        extract_tar(&archive, &extract).unwrap();

        let extracted_file = extract.join("test.txt");
        assert!(extracted_file.exists());
        assert_eq!(fs::read_to_string(&extracted_file).unwrap(), "test content");
    }

    #[test]
    fn tar_roundtrip_directory() {
        let dir = setup();
        let source = dir.path().join("source");
        let archive = dir.path().join("archive.tar");
        let extract = dir.path().join("extracted");

        fs::create_dir(&source).unwrap();
        fs::write(source.join("file1.txt"), "content1").unwrap();
        fs::write(source.join("file2.txt"), "content2").unwrap();

        create_tar(&source, &archive).unwrap();
        assert!(archive.exists());

        fs::create_dir(&extract).unwrap();
        extract_tar(&archive, &extract).unwrap();

        assert_eq!(
            fs::read_to_string(extract.join("file1.txt")).unwrap(),
            "content1"
        );
        assert_eq!(
            fs::read_to_string(extract.join("file2.txt")).unwrap(),
            "content2"
        );
    }

    #[test]
    fn tar_gz_roundtrip() {
        let dir = setup();
        let source = dir.path().join("source");
        let archive = dir.path().join("archive.tar.gz");
        let extract = dir.path().join("extracted");

        fs::create_dir(&source).unwrap();
        fs::write(source.join("file.txt"), "compressed content").unwrap();

        create_tar_gz(&source, &archive).unwrap();
        assert!(archive.exists());

        fs::create_dir(&extract).unwrap();
        extract_tar_gz(&archive, &extract).unwrap();

        assert_eq!(
            fs::read_to_string(extract.join("file.txt")).unwrap(),
            "compressed content"
        );
    }
}
