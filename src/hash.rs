//! SHA-256 digests as lowercase hex. Needs the `hash` feature.
//!
//! Use this module to check a downloaded file against a pinned digest.
//! It replaces `sha256sum`. It works on all platforms.
//!
//! - [`sha256`] hashes bytes in memory.
//! - [`sha256_file`] reads a file through one 64 KiB buffer. Memory use does
//!   not change with the file size.
//! - [`verify_sha256_file`] compares a file with an expected digest.
//!
//! An error names the path. A mismatch error shows the two digests.
//!
//! # Examples
//!
//! ```
//! use smop::hash;
//!
//! assert_eq!(
//!     hash::sha256(b"abc"),
//!     "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
//! );
//! ```
//!
//! ```no_run
//! use smop::hash;
//!
//! let pinned = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
//! hash::verify_sha256_file("target/tool.tar.gz", pinned)?;
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::fs::File;
use std::io::{self, ErrorKind, Read};
use std::path::Path;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

/// Read buffer size for files.
const BUF_LEN: usize = 64 * 1024;

/// Maximum number of characters of a bad `expected` in an error.
const SHOWN: usize = 80;

/// Returns the SHA-256 digest of `bytes` as 64 lowercase hex characters.
///
/// `bytes` can be `&[u8]`, `&str`, `String` or `Vec<u8>`.
///
/// # Examples
///
/// ```
/// let digest = smop::hash::sha256("abc");
/// assert!(digest.starts_with("ba7816bf"));
/// ```
#[must_use]
pub fn sha256<B: AsRef<[u8]>>(bytes: B) -> String {
    hex(&Sha256::digest(bytes))
}

/// Returns the SHA-256 digest of the file at `path` as lowercase hex.
///
/// This function reads the file through one 64 KiB buffer. It does not keep
/// the full file in memory.
///
/// # Errors
///
/// Returns an error that names the path if the file does not open or a read
/// fails. Examples: the file does not exist, or the path is a directory.
///
/// # Examples
///
/// ```no_run
/// let digest = smop::hash::sha256_file("Cargo.lock")?;
/// println!("{digest}  Cargo.lock");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn sha256_file<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    let file =
        File::open(path).with_context(|| format!("Failed to open file: {}", path.display()))?;
    sha256_reader(file).with_context(|| format!("Failed to read file: {}", path.display()))
}

/// Checks that the SHA-256 digest of the file at `path` is `expected`.
///
/// `expected` must be 64 hex characters, upper or lower case. Give only the
/// digest. A full `sha256sum` line or extra whitespace gives an error.
///
/// # Errors
///
/// Returns an error:
/// - If `expected` is not 64 hex characters. The function does not read the
///   file. The error quotes the first 80 characters of `expected`.
/// - If the file does not open or a read fails. The error names the path.
/// - If the digests are different:
///   `SHA-256 mismatch for <path>: expected <expected>, got <actual>`.
///
/// # Examples
///
/// ```no_run
/// let pinned = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
/// smop::hash::verify_sha256_file("downloads/empty.bin", pinned)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn verify_sha256_file<P: AsRef<Path>>(path: P, expected: &str) -> Result<()> {
    let path = path.as_ref();
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        // Show the start only. `expected` can be a full file.
        let shown: String = expected.chars().take(SHOWN).collect();
        let cut = if shown.len() < expected.len() {
            "..."
        } else {
            ""
        };
        bail!(
            "Invalid SHA-256 digest {shown:?}{cut} for {}: need 64 hex characters",
            path.display()
        );
    }
    let actual = sha256_file(path)?;
    if !actual.eq_ignore_ascii_case(expected) {
        bail!(
            "SHA-256 mismatch for {}: expected {expected}, got {actual}",
            path.display()
        );
    }
    Ok(())
}

/// Hashes `reader` to its end through one buffer. Memory use does not
/// change with the input size.
fn sha256_reader(mut reader: impl Read) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; BUF_LEN];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => return Ok(hex(&hasher.finalize())),
            Ok(n) => hasher.update(&buf[..n]),
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
}

/// Encodes `bytes` as lowercase hex. `sha2` 0.11's output type has no
/// `LowerHex`, so `format!("{:x}", ..)` does not compile.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(2 * bytes.len());
    for &b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 0x0f)]));
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn file_with(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.bin");
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    /// `len` bytes of `i % 251`, so no block repeats at a power of two.
    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| u8::try_from(i % 251).unwrap()).collect()
    }

    fn unhex(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }

    // NIST FIPS 180-2 vectors.
    #[test]
    fn sha256_of_empty_input() {
        assert_eq!(sha256(b""), EMPTY);
    }

    #[test]
    fn sha256_of_abc() {
        assert_eq!(sha256(b"abc"), ABC);
    }

    #[test]
    fn sha256_of_two_block_message() {
        assert_eq!(
            sha256("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn sha256_of_one_million_a() {
        assert_eq!(
            sha256(vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn sha256_takes_str_string_and_vec() {
        assert_eq!(sha256("abc"), ABC);
        assert_eq!(sha256(String::from("abc")), ABC);
        assert_eq!(sha256(Vec::from(*b"abc")), ABC);
        assert_eq!(sha256(*b"abc"), ABC);
    }

    #[test]
    fn sha256_file_of_empty_file() {
        let (_dir, path) = file_with(b"");
        assert_eq!(sha256_file(&path).unwrap(), EMPTY);
    }

    #[test]
    fn sha256_file_of_abc() {
        let (_dir, path) = file_with(b"abc");
        assert_eq!(sha256_file(&path).unwrap(), ABC);
    }

    #[test]
    fn sha256_file_at_buffer_boundaries() {
        for len in [1, BUF_LEN - 1, BUF_LEN, BUF_LEN + 1, 2 * BUF_LEN + 7] {
            let bytes = pattern(len);
            let (_dir, path) = file_with(&bytes);
            assert_eq!(sha256_file(&path).unwrap(), sha256(&bytes), "len {len}");
        }
    }

    #[test]
    fn sha256_file_missing_names_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.bin");
        let err = format!("{:#}", sha256_file(&path).unwrap_err());
        assert!(err.contains(&path.display().to_string()), "{err}");
    }

    #[test]
    fn sha256_file_of_directory_names_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let err = format!("{:#}", sha256_file(dir.path()).unwrap_err());
        assert!(err.contains(&dir.path().display().to_string()), "{err}");
    }

    /// Gives `data` in reads of at most 7 bytes. Every second read returns
    /// `Interrupted`. After `fail_at` bytes, a read gives another error.
    struct Flaky {
        data: Vec<u8>,
        pos: usize,
        calls: usize,
        fail_at: Option<usize>,
    }

    impl Read for Flaky {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.calls += 1;
            if self.calls.is_multiple_of(2) {
                return Err(io::Error::from(ErrorKind::Interrupted));
            }
            if self.fail_at.is_some_and(|at| self.pos >= at) {
                return Err(io::Error::other("disk gone"));
            }
            let n = buf.len().min(7).min(self.data.len() - self.pos);
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    #[test]
    fn sha256_reader_retries_interrupted_reads() {
        let data = pattern(1000);
        let reader = Flaky {
            data: data.clone(),
            pos: 0,
            calls: 0,
            fail_at: None,
        };
        assert_eq!(sha256_reader(reader).unwrap(), sha256(&data));
    }

    #[test]
    fn sha256_reader_stops_at_other_errors() {
        let reader = Flaky {
            data: pattern(1000),
            pos: 0,
            calls: 0,
            fail_at: Some(100),
        };
        let err = sha256_reader(reader).unwrap_err();
        assert_eq!(err.to_string(), "disk gone");
    }

    #[test]
    fn verify_accepts_matching_digest() {
        let (_dir, path) = file_with(b"abc");
        verify_sha256_file(&path, ABC).unwrap();
    }

    #[test]
    fn verify_accepts_uppercase_digest() {
        let (_dir, path) = file_with(b"abc");
        verify_sha256_file(&path, &ABC.to_uppercase()).unwrap();
    }

    #[test]
    fn verify_mismatch_shows_path_and_both_digests() {
        let (_dir, path) = file_with(b"abc");
        let err = format!("{:#}", verify_sha256_file(&path, EMPTY).unwrap_err());
        let want = format!(
            "SHA-256 mismatch for {}: expected {EMPTY}, got {ABC}",
            path.display()
        );
        assert_eq!(err, want);
    }

    #[test]
    fn verify_mismatch_shows_expected_as_given() {
        let (_dir, path) = file_with(b"abc");
        let upper = EMPTY.to_uppercase();
        let err = format!("{:#}", verify_sha256_file(&path, &upper).unwrap_err());
        assert!(
            err.contains(&format!("expected {upper}, got {ABC}")),
            "{err}"
        );
    }

    #[test]
    fn verify_rejects_malformed_expected_before_reading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.bin");
        let line = format!("{ABC}  data.bin\n");
        let newline = format!("{ABC}\n");
        let short = &ABC[..63];
        let long = format!("{ABC}0");
        let not_hex = "g".repeat(64);
        for bad in ["", "abc", &line, &newline, short, &long, &not_hex] {
            let err = format!("{:#}", verify_sha256_file(&path, bad).unwrap_err());
            assert!(err.contains("Invalid SHA-256 digest"), "{err}");
            assert!(err.contains(&format!("{bad:?}")), "{err}");
        }
    }

    #[test]
    fn verify_cuts_a_long_malformed_expected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.bin");
        let bad = "x".repeat(10_000);
        let err = format!("{:#}", verify_sha256_file(&path, &bad).unwrap_err());
        let shown = format!("{:?}...", "x".repeat(SHOWN));
        assert!(err.contains(&shown), "{err}");
        assert!(err.len() < 300, "{err}");
    }

    #[test]
    fn verify_missing_file_names_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.bin");
        let err = format!("{:#}", verify_sha256_file(&path, ABC).unwrap_err());
        assert!(err.contains(&path.display().to_string()), "{err}");
    }

    #[test]
    fn hex_of_known_bytes() {
        assert_eq!(hex(&[]), "");
        assert_eq!(hex(&[0x00, 0x0f, 0xa0, 0xff]), "000fa0ff");
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn hex_is_lowercase_and_round_trips(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
            let out = hex(&bytes);
            prop_assert_eq!(out.len(), 2 * bytes.len());
            prop_assert!(out.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
            prop_assert_eq!(unhex(&out), bytes);
        }

        #[test]
        fn sha256_is_64_lowercase_hex(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
            let out = sha256(&bytes);
            prop_assert_eq!(out.len(), 64);
            prop_assert!(out.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        }

        #[test]
        fn sha256_file_matches_sha256(bytes in proptest::collection::vec(any::<u8>(), 0..3 * BUF_LEN)) {
            let (_dir, path) = file_with(&bytes);
            prop_assert_eq!(sha256_file(&path).unwrap(), sha256(&bytes));
        }

        #[test]
        fn verify_agrees_with_sha256_file(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
            let (_dir, path) = file_with(&bytes);
            let digest = sha256(&bytes);
            prop_assert!(verify_sha256_file(&path, &digest).is_ok());
            let mut other = unhex(&digest);
            other[0] ^= 1;
            prop_assert!(verify_sha256_file(&path, &hex(&other)).is_err());
        }
    }
}
