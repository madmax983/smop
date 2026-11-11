//! SHA-256 digests as lowercase hex. Needs the `hash` feature.

use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

/// Read buffer size for files.
const BUF_LEN: usize = 64 * 1024;

/// Returns the SHA-256 digest of `bytes` as 64 lowercase hex characters.
#[must_use]
pub fn sha256(bytes: impl AsRef<[u8]>) -> String {
    hex(&Sha256::digest(bytes))
}

/// Returns the SHA-256 digest of the file at `path` as lowercase hex.
///
/// # Errors
///
/// Returns an error that names the path if the file can't be read.
pub fn sha256_file(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    let mut file =
        File::open(path).with_context(|| format!("Failed to open file: {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; BUF_LEN];
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("Failed to read file: {}", path.display()));
            }
        }
    }
    Ok(hex(&hasher.finalize()))
}

/// Checks that the SHA-256 digest of the file at `path` is `expected`.
///
/// # Errors
///
/// Returns an error if the file can't be read or the digests differ.
pub fn verify_sha256_file(path: impl AsRef<Path>, expected: &str) -> Result<()> {
    let path = path.as_ref();
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!(
            "Expected digest {expected:?} for {} is not a SHA-256 hex digest (64 hex characters)",
            path.display()
        );
    }
    let actual = sha256_file(path)?;
    if !actual.eq_ignore_ascii_case(expected) {
        bail!(
            "SHA-256 mismatch for {}: expected {}, got {actual}",
            path.display(),
            expected.to_ascii_lowercase()
        );
    }
    Ok(())
}

/// Encodes `bytes` as lowercase hex.
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
        assert_eq!(sha256(b"abc".to_vec()), ABC);
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
            let bytes: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            let (_dir, path) = file_with(&bytes);
            assert_eq!(sha256_file(&path).unwrap(), sha256(&bytes), "len {len}");
        }
    }

    #[test]
    fn sha256_file_of_10_mib_matches_sha256() {
        let bytes: Vec<u8> = (0..10 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        let (_dir, path) = file_with(&bytes);
        assert_eq!(sha256_file(&path).unwrap(), sha256(&bytes));
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
        assert!(err.contains(&path.display().to_string()), "{err}");
        assert!(err.contains(EMPTY), "{err}");
        assert!(err.contains(ABC), "{err}");
    }

    #[test]
    fn verify_rejects_malformed_expected_before_reading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.bin");
        for bad in ["", "abc", &format!("{ABC}  data.bin\n"), &"g".repeat(64)] {
            let err = format!("{:#}", verify_sha256_file(&path, bad).unwrap_err());
            assert!(err.contains("not a SHA-256 hex digest"), "{err}");
            assert!(err.contains(&format!("{bad:?}")), "{err}");
        }
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
