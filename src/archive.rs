//! Archive creation and extraction (ZIP, TAR, TAR.GZ).
//!
//! Functions for working with compressed archives.
//! This module is only available with the `archive` feature.
//!
//! # Safe extraction
//!
//! The `extract_*` functions use the same rules for all formats (ADR 0008).
//! In this section, a *link* is a symbolic link.
//!
//! - An entry with an absolute path is an error.
//! - `a/../b.txt` extracts to `dest/b.txt`. An entry that goes above `dest`,
//!   such as `../b.txt`, is an error.
//! - A link entry that points outside `dest`, or to an absolute path, is an
//!   error. No such link stays on disk.
//! - No write follows a link out of `dest`. This includes links that were
//!   in `dest` before the extraction.
//!
//! The error names the entry. If an error occurs, `dest` can contain some of
//! the entries.

mod contain;

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use contain::Extractor;

/// Extracts a ZIP archive to the specified directory.
///
/// Makes `dest` if it does not exist. Link entries become links. See
/// [Safe extraction](self#safe-extraction) for the path rules.
///
/// # Errors
///
/// Returns an error if it cannot read the archive, if a write fails, or if
/// an entry is unsafe. An error for an unsafe entry names the entry.
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

    let file = File::open(archive)
        .with_context(|| format!("Failed to open ZIP archive: {}", archive.display()))?;

    let mut zip = zip::ZipArchive::new(file)
        .with_context(|| format!("Failed to read ZIP archive: {}", archive.display()))?;

    Extractor::run(archive, dest.as_ref(), |x| {
        for i in 0..zip.len() {
            let mut file = zip.by_index(i).with_context(|| {
                format!("Failed to read entry {} from ZIP: {}", i, archive.display())
            })?;
            // Some ZIP tools write `\` as the separator.
            let name = PathBuf::from(zip_path(&file.name().with_context(|| {
                format!("Failed to read entry {} from ZIP: {}", i, archive.display())
            })?));

            if file.is_dir() {
                if let Some(path) = x.dir(&name)? {
                    fs::create_dir_all(&path).with_context(|| {
                        format!("Failed to create directory: {}", path.display())
                    })?;
                }
            } else if file.is_symlink() {
                let mut target = String::new();
                file.read_to_string(&mut target)
                    .with_context(|| format!("Failed to read link target: {}", name.display()))?;
                x.symlink(&name, Path::new(&zip_path(&target)))?;
            } else {
                let path = x.file(&name)?;
                let mut out = contain::create_file(&path)?;
                std::io::copy(&mut file, &mut out)
                    .with_context(|| format!("Failed to extract file: {}", path.display()))?;
            }
        }
        Ok(())
    })
}

/// Changes each `\` in a ZIP name to `/`.
fn zip_path(name: &str) -> String {
    name.replace('\\', "/")
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
/// Makes `dest` if it does not exist. Link and hard link entries become
/// links. Extraction keeps file modes and times. See
/// [Safe extraction](self#safe-extraction) for the path rules.
///
/// # Errors
///
/// Returns an error if it cannot read the archive, if a write fails, or if
/// an entry is unsafe. An error for an unsafe entry names the entry.
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

    let file = File::open(archive)
        .with_context(|| format!("Failed to open TAR archive: {}", archive.display()))?;

    unpack_tar(tar::Archive::new(file), archive, dest.as_ref())
}

/// Extracts a TAR stream one entry at a time, with the rules of
/// [`Extractor`].
///
/// Directory entries are applied last, in reverse path order, as
/// `tar::Archive::unpack` does. A child directory comes before its parent.
/// Then a read-only directory mode does not stop writes to the files in it.
fn unpack_tar<R: Read>(mut tar: tar::Archive<R>, archive: &Path, dest: &Path) -> Result<()> {
    let read_err = || format!("Failed to read TAR archive: {}", archive.display());
    Extractor::run(archive, dest, |x| {
        let mut dirs = Vec::new();
        for entry in tar.entries().with_context(read_err)? {
            let mut entry = entry.with_context(read_err)?;
            let name = entry.path().with_context(read_err)?.into_owned();
            let kind = entry.header().entry_type();
            let link = kind.is_symlink() || kind.is_hard_link();
            let meta = kind.is_pax_global_extensions()
                || kind.is_pax_local_extensions()
                || kind.is_gnu_longname()
                || kind.is_gnu_longlink();
            // `tar` makes a directory for an old (not ustar) header with a
            // name that ends in `/`, whatever its type. Delay it as a
            // directory too.
            let old_dir = !link
                && !meta
                && entry.header().as_ustar().is_none()
                && entry.path_bytes().ends_with(b"/");

            if meta {
                // Metadata only. `entries()` applies it to the next entry.
            } else if kind.is_dir() || old_dir {
                if let Some(path) = x.dir(&name)? {
                    dirs.push((path, name, entry));
                }
            } else if link {
                let target = entry
                    .link_name()
                    .with_context(read_err)?
                    .ok_or_else(|| {
                        anyhow!(
                            "Failed to read link target of entry \"{}\" in archive {}",
                            name.display(),
                            archive.display()
                        )
                    })?
                    .into_owned();
                if kind.is_symlink() {
                    x.symlink(&name, &target)?;
                } else {
                    x.hard_link(&name, &target)?;
                }
            } else {
                let path = x.file(&name)?;
                // `unpack` replaces a file or link at `path`. It does not
                // write through it.
                entry
                    .unpack(&path)
                    .with_context(|| format!("Failed to extract file: {}", path.display()))?;
            }
        }

        // Check the links before the directory modes are set. A read-only
        // directory can stop the removal of an unsafe link.
        x.check_links()?;

        dirs.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, name, mut entry) in dirs {
            // Find the path again: a later link entry can change it.
            if let Some(path) = x.dir(&name)? {
                entry
                    .unpack(&path)
                    .with_context(|| format!("Failed to create directory: {}", path.display()))?;
            }
        }
        Ok(())
    })
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
/// Makes `dest` if it does not exist. Link and hard link entries become
/// links. Extraction keeps file modes and times. See
/// [Safe extraction](self#safe-extraction) for the path rules.
///
/// # Errors
///
/// Returns an error if it cannot read the archive, if a write fails, or if
/// an entry is unsafe. An error for an unsafe entry names the entry.
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

    let file = File::open(archive)
        .with_context(|| format!("Failed to open TAR.GZ archive: {}", archive.display()))?;

    let decompressor = flate2::read::GzDecoder::new(file);
    unpack_tar(tar::Archive::new(decompressor), archive, dest.as_ref())
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
