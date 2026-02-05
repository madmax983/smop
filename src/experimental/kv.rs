//! Persistent Key-Value Store
//!
//! A simple JSON-backed key-value store for keeping state between script runs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::fs;

/// A persistent key-value store backed by a JSON file.
pub struct Store {
    path: PathBuf,
    data: HashMap<String, Value>,
}

impl Store {
    /// Opens a store at the given path.
    ///
    /// If the file exists, it loads the data.
    /// If it doesn't exist, it starts with an empty store (created on first write).
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let data = if path.exists() {
            fs::read_json(&path)?
        } else {
            HashMap::new()
        };
        Ok(Self { path, data })
    }

    /// Gets a value from the store.
    ///
    /// # Errors
    ///
    /// Returns an error if the value cannot be deserialized into type `T`.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        if let Some(value) = self.data.get(key) {
            let t: T = serde_json::from_value(value.clone())?;
            Ok(Some(t))
        } else {
            Ok(None)
        }
    }

    /// Sets a value in the store and saves it to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the value cannot be serialized or the file cannot be written.
    pub fn set<T: Serialize>(&mut self, key: &str, value: &T) -> Result<()> {
        let v = serde_json::to_value(value)?;
        self.data.insert(key.to_string(), v);
        self.save()
    }

    /// Gets a value, or computes and sets it if missing.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The value exists but cannot be deserialized.
    /// - The value needs to be computed and `f` fails.
    /// - The computed value cannot be serialized or saved.
    pub fn get_or<T, F>(&mut self, key: &str, f: F) -> Result<T>
    where
        T: Serialize + DeserializeOwned + Clone,
        F: FnOnce() -> Result<T>,
    {
        if let Some(val) = self.get(key)? {
            Ok(val)
        } else {
            let val = f()?;
            self.set(key, &val)?;
            Ok(val)
        }
    }

    /// Saves the current store to disk.
    fn save(&self) -> Result<()> {
        fs::write_json(&self.path, &self.data)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_store_persistence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("store.json");

        // Open and write
        {
            let mut db = Store::open(&path).unwrap();
            db.set("counter", &42).unwrap();
        }

        // Re-open and read
        {
            let db = Store::open(&path).unwrap();
            let val: i32 = db.get("counter").unwrap().unwrap();
            assert_eq!(val, 42);
        }
    }

    #[test]
    fn test_get_or() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cache.json");
        let mut db = Store::open(&path).unwrap();

        let val = db.get_or("key", || Ok("computed".to_string())).unwrap();
        assert_eq!(val, "computed");

        // Should return cached value
        let val2: String = db.get_or("key", || panic!("Should not recompute")).unwrap();
        assert_eq!(val2, "computed");
    }
}
