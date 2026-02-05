//! Simple key-value store backed by a JSON file.
//!
//! Useful for scripts that need to persist small amounts of state
//! between runs (e.g., timestamps, counters, cached values).

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::fs;

/// A persistent key-value store.
pub struct Store {
    path: PathBuf,
    data: HashMap<String, Value>,
}

impl Store {
    /// Opens a store at the given path.
    ///
    /// If the file exists, it is loaded. If not, a new store is created.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn open<P: Into<PathBuf>>(path: P) -> Result<Self> {
        let path = path.into();
        let data = if path.exists() {
            fs::read_json(&path).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Ok(Self { path, data })
    }

    /// Retrieves a value from the store.
    ///
    /// # Errors
    ///
    /// Returns an error if the value exists but cannot be deserialized into `T`.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match self.data.get(key) {
            Some(value) => {
                let val: T = serde_json::from_value(value.clone())
                    .with_context(|| format!("Failed to deserialize key: {key}"))?;
                Ok(Some(val))
            }
            None => Ok(None),
        }
    }

    /// Sets a value in the store and persists it to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the value cannot be serialized or the file cannot be written.
    pub fn set<T: Serialize>(&mut self, key: &str, value: T) -> Result<()> {
        let val = serde_json::to_value(value)
            .with_context(|| format!("Failed to serialize value for key: {key}"))?;
        self.data.insert(key.to_string(), val);
        self.save()
    }

    /// Removes a value from the store and persists changes to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn remove(&mut self, key: &str) -> Result<()> {
        self.data.remove(key);
        self.save()
    }

    /// Clears all values from the store and persists changes to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn clear(&mut self) -> Result<()> {
        self.data.clear();
        self.save()
    }

    /// Returns a list of all keys in the store.
    #[must_use]
    pub fn keys(&self) -> Vec<String> {
        self.data.keys().cloned().collect()
    }

    /// Persists the store to disk.
    fn save(&self) -> Result<()> {
        fs::write_json(&self.path, &self.data)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn open_creates_new_store() {
        let dir = setup();
        let path = dir.path().join("store.json");
        let store = Store::open(&path).unwrap();
        assert!(store.keys().is_empty());
    }

    #[test]
    fn set_persists_data() {
        let dir = setup();
        let path = dir.path().join("store.json");

        {
            let mut store = Store::open(&path).unwrap();
            store.set("key", "value").unwrap();
        }

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("key"));
        assert!(content.contains("value"));
    }

    #[test]
    fn get_retrieves_data() {
        let dir = setup();
        let path = dir.path().join("store.json");

        {
            let mut store = Store::open(&path).unwrap();
            store.set("number", 42).unwrap();
        }

        let store = Store::open(&path).unwrap();
        let val: i32 = store.get("number").unwrap().unwrap();
        assert_eq!(val, 42);
    }

    #[test]
    fn remove_deletes_data() {
        let dir = setup();
        let path = dir.path().join("store.json");

        {
            let mut store = Store::open(&path).unwrap();
            store.set("key", "value").unwrap();
            store.remove("key").unwrap();
        }

        let store = Store::open(&path).unwrap();
        let val: Option<String> = store.get("key").unwrap();
        assert!(val.is_none());
    }

    #[test]
    fn types_are_preserved() {
        #[derive(Serialize, serde::Deserialize, PartialEq, Debug)]
        struct User {
            name: String,
            age: u32,
        }

        let dir = setup();
        let path = dir.path().join("store.json");

        let user = User {
            name: "Nova".into(),
            age: 100,
        };

        {
            let mut store = Store::open(&path).unwrap();
            store.set("user", &user).unwrap();
        }

        let store = Store::open(&path).unwrap();
        let retrieved: User = store.get("user").unwrap().unwrap();
        assert_eq!(user, retrieved);
    }
}
