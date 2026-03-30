//! Script execution for validated `smop` scripts.

use anyhow::{Context, Result};

use super::validate::{StepKind, ValidatedScript};

/// Executes a validated script sequentially.
///
/// Steps run in order and stop on the first error.
pub fn execute_script(script: &ValidatedScript) -> Result<()> {
    let total = script.steps.len();

    for (index, step) in script.steps.iter().enumerate() {
        eprintln!("Running step {}/{}: {}", index + 1, total, step.name);
        execute_step(&step.kind).with_context(|| format!("Step '{}' failed", step.name))?;
    }

    Ok(())
}

fn execute_step(kind: &StepKind) -> Result<()> {
    match kind {
        StepKind::EnvRequire { vars } => {
            let names: Vec<&str> = vars.iter().map(String::as_str).collect();
            crate::env::require_vars(&names)
        }
        StepKind::FsMkdirAll { path } => std::fs::create_dir_all(path)
            .with_context(|| format!("Failed to create directory: {path}")),
        StepKind::FsWriteString { path, content } => crate::fs::write_string(path, content),
        StepKind::FsAppend { path, content } => crate::fs::append(path, content),
        StepKind::FsCopy { from, to } => crate::fs::copy(from, to),
        StepKind::FsRename { from, to } => crate::fs::rename(from, to),
        StepKind::FsRemove { path } => crate::fs::remove(path),
        StepKind::HttpDownload { url, path } => execute_http_download(url, path),
        StepKind::HttpGet { url, dest } => execute_http_get(url, dest),
        StepKind::ArchiveCreateZip { source, dest } => execute_archive_create_zip(source, dest),
        StepKind::ArchiveCreateTar { source, dest } => execute_archive_create_tar(source, dest),
        StepKind::ArchiveCreateTarGz { source, dest } => {
            execute_archive_create_tar_gz(source, dest)
        }
        StepKind::ArchiveExtractZip { archive, dest } => execute_archive_extract_zip(archive, dest),
        StepKind::ArchiveExtractTar { archive, dest } => execute_archive_extract_tar(archive, dest),
        StepKind::ArchiveExtractTarGz { archive, dest } => {
            execute_archive_extract_tar_gz(archive, dest)
        }
        StepKind::ShRun { command } => crate::sh::run(command),
    }
}

fn execute_http_download(url: &str, path: &str) -> Result<()> {
    #[cfg(feature = "http")]
    {
        return crate::http::download(url, path);
    }

    #[cfg(not(feature = "http"))]
    {
        let _ = (url, path);
        bail!("http feature is not enabled");
    }
}

fn execute_http_get(url: &str, dest: &str) -> Result<()> {
    #[cfg(feature = "http")]
    {
        let body = crate::http::get(url)?;
        crate::fs::write_string(dest, body)
    }

    #[cfg(not(feature = "http"))]
    {
        let _ = (url, dest);
        bail!("http feature is not enabled");
    }
}

fn execute_archive_create_zip(source: &str, dest: &str) -> Result<()> {
    #[cfg(feature = "archive")]
    {
        crate::archive::create_zip(source, dest)
    }

    #[cfg(not(feature = "archive"))]
    {
        let _ = (source, dest);
        bail!("archive feature is not enabled");
    }
}

fn execute_archive_create_tar(source: &str, dest: &str) -> Result<()> {
    #[cfg(feature = "archive")]
    {
        crate::archive::create_tar(source, dest)
    }

    #[cfg(not(feature = "archive"))]
    {
        let _ = (source, dest);
        bail!("archive feature is not enabled");
    }
}

fn execute_archive_create_tar_gz(source: &str, dest: &str) -> Result<()> {
    #[cfg(feature = "archive")]
    {
        crate::archive::create_tar_gz(source, dest)
    }

    #[cfg(not(feature = "archive"))]
    {
        let _ = (source, dest);
        bail!("archive feature is not enabled");
    }
}

fn execute_archive_extract_zip(archive: &str, dest: &str) -> Result<()> {
    #[cfg(feature = "archive")]
    {
        crate::archive::extract_zip(archive, dest)
    }

    #[cfg(not(feature = "archive"))]
    {
        let _ = (archive, dest);
        bail!("archive feature is not enabled");
    }
}

fn execute_archive_extract_tar(archive: &str, dest: &str) -> Result<()> {
    #[cfg(feature = "archive")]
    {
        crate::archive::extract_tar(archive, dest)
    }

    #[cfg(not(feature = "archive"))]
    {
        let _ = (archive, dest);
        bail!("archive feature is not enabled");
    }
}

fn execute_archive_extract_tar_gz(archive: &str, dest: &str) -> Result<()> {
    #[cfg(feature = "archive")]
    {
        crate::archive::extract_tar_gz(archive, dest)
    }

    #[cfg(not(feature = "archive"))]
    {
        let _ = (archive, dest);
        bail!("archive feature is not enabled");
    }
}
