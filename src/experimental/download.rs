//! Download utilities.

use anyhow::{Context, Result, anyhow};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

#[cfg(feature = "print")]
use indicatif::{ProgressBar, ProgressStyle};

/// Downloads a file from a URL to a path, with a progress bar.
///
/// # Errors
///
/// Returns an error if the download fails or file cannot be written.
pub fn download_file<P: AsRef<Path>>(url: &str, path: P) -> Result<()> {
    let path = path.as_ref();

    // 1. Make request
    let response = ureq::get(url)
        .call()
        .map_err(|e| anyhow!("Failed to fetch {url}: {e}"))?;

    // 2. Get content length
    let total_size = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    // 3. Open file
    let mut file =
        File::create(path).with_context(|| format!("Failed to create file: {}", path.display()))?;

    // 4. Setup progress bar (if enabled)
    #[cfg(feature = "print")]
    let pb = {
        let pb = total_size.map_or_else(
            || {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner());
                pb
            },
            |size| {
                let pb = ProgressBar::new(size);
                #[allow(clippy::literal_string_with_formatting_args)]
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
                        .unwrap_or_else(|_| ProgressStyle::default_bar())
                        .progress_chars("#>-"),
                );
                pb
            },
        );
        pb.set_message(format!(
            "Downloading {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        Some(pb)
    };

    // 5. Read and write
    let mut reader = response.into_body().into_reader();

    let mut buffer = [0; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| "Failed to read from response")?;

        if read == 0 {
            break;
        }

        file.write_all(&buffer[..read])
            .with_context(|| "Failed to write to file")?;

        #[cfg(feature = "print")]
        if let Some(ref pb) = pb {
            pb.inc(read as u64);
        }
    }

    #[cfg(feature = "print")]
    if let Some(pb) = pb {
        pb.finish_with_message("Download complete");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    #[allow(clippy::unwrap_used)]
    #[allow(clippy::significant_drop_tightening)]
    fn download_file_fetches_and_saves() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("GET", "/test.txt")
            .with_status(200)
            .with_body("Hello, Nova!")
            .create();

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("downloaded.txt");
        let url = format!("{}/test.txt", server.url());

        download_file(&url, &path).unwrap();

        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "Hello, Nova!");
    }
}
