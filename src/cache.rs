//! Persistent cache utilities.
//!
//! Allows caching results of expensive operations (like HTTP requests) to disk
//! with a specified TTL.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

use crate::fs;

/// Runs a closure and caches the result for the specified duration.
///
/// If the cache entry exists and is fresh, the cached value is returned.
/// Otherwise, the closure is executed, and the result is saved to the cache.
///
/// The cache is stored in the system's user cache directory under `smop`.
/// The filename is derived from the SHA256 hash of the key.
///
/// # Errors
///
/// Returns an error if:
/// - The cache directory cannot be determined.
/// - The cache file cannot be read or written.
/// - The closure returns an error.
/// - Serialization/deserialization fails.
///
/// # Examples
///
/// ```no_run
/// use smop::cache;
/// use std::time::Duration;
///
/// fn fetch_data() -> anyhow::Result<String> {
///     // simulate network request
///     Ok("data".to_string())
/// }
///
/// let result: String = cache::run("my-unique-key", Duration::from_secs(60), || {
///     fetch_data()
/// }).unwrap();
/// ```
pub fn run<T, F>(key: &str, ttl: Duration, op: F) -> Result<T>
where
    T: Serialize + DeserializeOwned,
    F: FnOnce() -> Result<T>,
{
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow!("Could not determine cache directory"))?
        .join("smop");

    run_in_dir(&cache_dir, key, ttl, op)
}

/// Clears the cache entry for the given key.
///
/// # Errors
///
/// Returns an error if the file exists but cannot be removed.
pub fn clear(key: &str) -> Result<()> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow!("Could not determine cache directory"))?
        .join("smop");

    let hash = format!("{:x}", Sha256::digest(key));
    let path = cache_dir.join(format!("{hash}.json"));

    if path.exists() {
        fs::remove(&path)?;
    }
    Ok(())
}

/// Clears all cache entries.
///
/// # Errors
///
/// Returns an error if the cache directory cannot be cleaned.
pub fn clear_all() -> Result<()> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow!("Could not determine cache directory"))?
        .join("smop");

    if cache_dir.exists() {
        fs::remove(&cache_dir)?;
    }
    Ok(())
}

/// Internal implementation that allows injecting the cache directory for testing.
fn run_in_dir<T, F>(cache_dir: &Path, key: &str, ttl: Duration, op: F) -> Result<T>
where
    T: Serialize + DeserializeOwned,
    F: FnOnce() -> Result<T>,
{
    if !cache_dir.exists() {
        std::fs::create_dir_all(cache_dir).with_context(|| {
            format!("Failed to create cache directory: {}", cache_dir.display())
        })?;
    }

    let hash = format!("{:x}", Sha256::digest(key));
    let path = cache_dir.join(format!("{hash}.json"));

    if path.exists() {
        // Check metadata for modification time
        let metadata = std::fs::metadata(&path)
            .with_context(|| format!("Failed to read metadata for: {}", path.display()))?;

        #[allow(clippy::collapsible_if)]
        if let Ok(modified) = metadata.modified() {
            if modified.elapsed().map(|e| e < ttl).unwrap_or(false) {
                // Cache hit
                // Try to read. If it fails (corrupt), fall through to re-run.
                if let Ok(val) = fs::read_json::<T, _>(&path) {
                    return Ok(val);
                }
            }
        }
    }

    // Cache miss, expired, or corrupt
    let value = op()?;
    fs::write_json(&path, &value)?;

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_cache_hit_and_expiry() {
        let temp = tempfile::TempDir::new().unwrap();
        let dir = temp.path();
        let key = "test_key";

        // First run: execute op
        let val: i32 = run_in_dir(dir, key, Duration::from_secs(1), || Ok(1)).unwrap();
        assert_eq!(val, 1);

        // Second run: should hit cache (op returns 2, but we get 1)
        let val: i32 = run_in_dir(dir, key, Duration::from_secs(1), || Ok(2)).unwrap();
        assert_eq!(val, 1);

        // Wait for expiry
        thread::sleep(Duration::from_millis(1100));

        // Third run: expired, execute op (returns 3)
        let val: i32 = run_in_dir(dir, key, Duration::from_secs(1), || Ok(3)).unwrap();
        assert_eq!(val, 3);
    }

    #[test]
    fn test_different_keys() {
        let temp = tempfile::TempDir::new().unwrap();
        let dir = temp.path();

        let val1: i32 = run_in_dir(dir, "key1", Duration::from_secs(10), || Ok(10)).unwrap();
        let val2: i32 = run_in_dir(dir, "key2", Duration::from_secs(10), || Ok(20)).unwrap();

        assert_eq!(val1, 10);
        assert_eq!(val2, 20);
    }
}
