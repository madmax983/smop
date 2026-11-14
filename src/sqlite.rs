//! SQLite stub (RED phase).

use std::path::Path;

use anyhow::{Result, bail};
use serde::de::DeserializeOwned;

#[doc(no_inline)]
pub use rusqlite::{Connection, Params, named_params, params};

/// Stub.
///
/// # Errors
///
/// Always.
pub fn open<P: AsRef<Path>>(_path: P) -> Result<Connection> {
    bail!("not implemented")
}

/// Stub.
///
/// # Errors
///
/// Always.
pub fn open_read_only<P: AsRef<Path>>(_path: P) -> Result<Connection> {
    bail!("not implemented")
}

/// Stub.
///
/// # Errors
///
/// Always.
pub fn execute<P: Params>(_db: &Connection, _sql: &str, _params: P) -> Result<usize> {
    bail!("not implemented")
}

/// Stub.
///
/// # Errors
///
/// Always.
pub fn query<T: DeserializeOwned, P: Params>(
    _db: &Connection,
    _sql: &str,
    _params: P,
) -> Result<Vec<T>> {
    bail!("not implemented")
}

/// Stub.
///
/// # Errors
///
/// Always.
pub fn query_one<T: DeserializeOwned, P: Params>(
    _db: &Connection,
    _sql: &str,
    _params: P,
) -> Result<T> {
    bail!("not implemented")
}
