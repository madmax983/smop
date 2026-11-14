//! SQLite databases. Needs the `sqlite` feature.
//!
//! This module uses [`rusqlite`] with SQLite bundled. You do
//! not need SQLite on the system.
//!
//! - [`open`] opens a database file and makes it if it does not exist.
//!   `":memory:"` gives an in-memory database.
//! - [`open_read_only`] opens an existing file for reads only. It can read
//!   next to a live writer.
//! - [`execute`] runs one statement and returns the number of changed rows.
//! - [`query`] runs one statement and deserializes each row into `T`.
//!   [`query_one`] gives the first row.
//!
//! A connection is a [`Connection`]. Use its methods for transactions and
//! for a script of many statements (`db.execute_batch(sql)`).
//!
//! # Rows into types
//!
//! | `T` | Columns |
//! |-----|---------|
//! | struct, map, `serde_json::Value` | by name; other columns are ignored |
//! | tuple | by position; the counts must be equal |
//! | scalar, `Option`, enum, newtype | the row must have one column |
//!
//! | SQLite value | Rust types |
//! |--------------|------------|
//! | `INTEGER` | integers (range checked), floats, `bool` (0 is `false`) |
//! | `REAL` | `f64`, `f32` |
//! | `TEXT` | `String`, `char`, unit enum variants by name |
//! | `BLOB` | `Vec<u8>` |
//! | `NULL` | `None` for an `Option`; other types give an error |
//!
//! Use `AS` to give a column the name of a field:
//! `SELECT count(*) AS total`.
//!
//! # Errors
//!
//! An SQL error quotes the statement: `SQL error in "SELECT * FROM nope"`.
//! The cause is the [`rusqlite::Error`]. The error does
//! not show parameter values, so give secrets as parameters, not in the SQL
//! text. A row error gives the row number and the column:
//! `Failed to read row 2 of "SELECT ..."`, then
//! `column "age": invalid type: NULL, expected u32`.
//!
//! # Build cost
//!
//! The `bundled` option compiles the SQLite C source with a C compiler.
//! A Rust toolchain has one on all platforms. The feature adds 14 crates.
//! On a 4-core Linux machine, a clean debug build takes 6 s more. A clean
//! release build takes about 50 s. The C source is one file, so more cores
//! do not make it faster.
//!
//! # Examples
//!
//! ```
//! use smop::prelude::*;
//!
//! #[derive(Deserialize)]
//! # #[serde(crate = "smop::serde")]
//! struct Task {
//!     id: i64,
//!     title: String,
//!     done: bool,
//! }
//!
//! let db = sqlite::open(":memory:")?;
//! sqlite::execute(&db, "CREATE TABLE task (id INTEGER PRIMARY KEY, title TEXT, done INTEGER)", ())?;
//! sqlite::execute(&db, "INSERT INTO task (title, done) VALUES (?1, ?2)", ("write docs", false))?;
//!
//! let open: Vec<Task> = sqlite::query(&db, "SELECT * FROM task WHERE NOT done", ())?;
//! assert_eq!(open[0].title, "write docs");
//! assert!(!open[0].done && open[0].id == 1);
//!
//! let total: i64 = sqlite::query_one(&db, "SELECT count(*) FROM task", ())?;
//! assert_eq!(total, 1);
//! # Ok::<(), anyhow::Error>(())
//! ```

mod de;

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use rusqlite::OpenFlags;
use rusqlite::types::Value;
use serde::de::DeserializeOwned;

/// A connection to a database. `rusqlite::Connection`.
#[doc(no_inline)]
pub use rusqlite::Connection;

/// The parameters of a statement: `()`, a tuple, an array, [`params!`] or
/// [`named_params!`].
#[doc(no_inline)]
pub use rusqlite::Params;

#[doc(no_inline)]
pub use rusqlite::{named_params, params};

/// The statement text for an error message.
fn quoted(sql: &str) -> String {
    format!("{:?}", sql.trim())
}

/// Adds the statement to a rusqlite error.
fn sql_error(sql: &str) -> impl Fn(rusqlite::Error) -> anyhow::Error + '_ {
    move |err| anyhow::Error::new(err).context(format!("SQL error in {}", quoted(sql)))
}

/// Opens the database at `path` for reads and writes. Makes the file if it
/// does not exist. `":memory:"` gives a new in-memory database.
///
/// This function does not make parent directories. The connection waits up
/// to 5 s for a lock that another connection holds.
///
/// # Errors
///
/// Returns an error that names the path if SQLite cannot open the file.
/// Examples: the directory does not exist, or the file is not a database.
///
/// # Examples
///
/// ```no_run
/// let db = smop::sqlite::open("target/app.db")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn open<P: AsRef<Path>>(path: P) -> Result<Connection> {
    let path = path.as_ref();
    Connection::open(path)
        .and_then(check)
        .with_context(|| format!("Failed to open SQLite database: {}", path.display()))
}

/// Opens the existing database at `path` for reads only.
///
/// Use it to read a database that another process writes. Each statement
/// sees the last committed data. A write gives an error.
///
/// # Errors
///
/// Returns an error that names the path if the file does not exist or is
/// not a database. This function does not make a file.
///
/// # Examples
///
/// ```no_run
/// let db = smop::sqlite::open_read_only("ledger.db")?;
/// let n: i64 = smop::sqlite::query_one(&db, "SELECT count(*) FROM entry", ())?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn open_read_only<P: AsRef<Path>>(path: P) -> Result<Connection> {
    let path = path.as_ref();
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    Connection::open_with_flags(path, flags)
        .and_then(check)
        .with_context(|| {
            format!(
                "Failed to open SQLite database for reads: {}",
                path.display()
            )
        })
}

/// SQLite reads the file at the first statement. Read the schema now, so
/// that a file that is not a database gives an error at open.
fn check(db: Connection) -> rusqlite::Result<Connection> {
    db.query_row("SELECT count(*) FROM sqlite_schema", (), |_| Ok(()))?;
    Ok(db)
}

/// Runs one SQL statement. Returns the number of rows that it changed.
///
/// Give values as `params`, not in the SQL text: `()`, a tuple, an array,
/// [`params!`] or [`named_params!`]. For a statement that returns rows, use
/// [`query`].
///
/// # Errors
///
/// Returns an error that quotes the statement if it is not valid SQL, if a
/// constraint fails, if the parameters do not agree with the statement, if
/// `sql` has more than one statement, or if the statement returns rows.
///
/// # Examples
///
/// ```
/// use smop::sqlite;
///
/// let db = sqlite::open(":memory:")?;
/// sqlite::execute(&db, "CREATE TABLE kv (k TEXT PRIMARY KEY, v TEXT)", ())?;
/// let n = sqlite::execute(&db, "INSERT INTO kv VALUES (?1, ?2)", ("a", "1"))?;
/// assert_eq!(n, 1);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn execute<P: Params>(db: &Connection, sql: &str, params: P) -> Result<usize> {
    db.execute(sql, params).map_err(sql_error(sql))
}

/// Runs one SQL statement and deserializes each row into `T`.
///
/// See the [module documentation](self) for how columns go into `T`.
/// `INSERT ... RETURNING` also works.
///
/// # Errors
///
/// Returns an error that quotes the statement if the statement fails (see
/// [`execute`]). Returns an error with the row number, the statement and
/// the column if a row does not go into `T`.
///
/// # Examples
///
/// ```
/// use smop::sqlite;
///
/// let db = sqlite::open(":memory:")?;
/// let rows: Vec<(i64, String)> =
///     sqlite::query(&db, "SELECT 1, 'a' UNION ALL SELECT 2, 'b'", ())?;
/// assert_eq!(rows, [(1, "a".to_string()), (2, "b".to_string())]);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn query<T: DeserializeOwned, P: Params>(
    db: &Connection,
    sql: &str,
    params: P,
) -> Result<Vec<T>> {
    rows(db, sql, params, usize::MAX)
}

/// Runs one SQL statement and deserializes its first row into `T`. Other
/// rows are not read.
///
/// # Errors
///
/// Returns `No rows from "<statement>"` if there is no row. Other errors
/// are as for [`query`].
///
/// # Examples
///
/// ```
/// use smop::sqlite;
///
/// let db = sqlite::open(":memory:")?;
/// let n: i64 = sqlite::query_one(&db, "SELECT 40 + 2", ())?;
/// assert_eq!(n, 42);
/// let none: Option<i64> = sqlite::query_one(&db, "SELECT NULL", ())?;
/// assert_eq!(none, None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn query_one<T: DeserializeOwned, P: Params>(
    db: &Connection,
    sql: &str,
    params: P,
) -> Result<T> {
    rows(db, sql, params, 1)?
        .pop()
        .ok_or_else(|| anyhow!("No rows from {}", quoted(sql)))
}

/// Reads at most `max` rows of `sql` into `T`.
fn rows<T: DeserializeOwned, P: Params>(
    db: &Connection,
    sql: &str,
    params: P,
    max: usize,
) -> Result<Vec<T>> {
    let fail = sql_error(sql);
    let mut stmt = db.prepare(sql).map_err(&fail)?;
    let names: Vec<String> = stmt.column_names().map(String::from).collect();
    let mut rows = stmt.query(params).map_err(&fail)?;
    let mut out = Vec::new();
    while out.len() < max {
        let Some(row) = rows.next().map_err(&fail)? else {
            break;
        };
        let n = out.len() + 1;
        let at = || format!("Failed to read row {n} of {}", quoted(sql));
        let values = (0..names.len())
            .map(|i| row.get::<_, Value>(i))
            .collect::<rusqlite::Result<Vec<_>>>()
            .with_context(at)?;
        let row = de::Row {
            names: &names,
            values,
        };
        out.push(T::deserialize(row).with_context(at)?);
    }
    Ok(out)
}
