//! SQLite databases. Needs the `sqlite` feature. `full` has it; the
//! default features do not.
//!
//! This module uses [`rusqlite`] with SQLite bundled. You do
//! not need SQLite on the system.
//!
//! - [`open`] opens a database file and makes it if it does not exist.
//!   `":memory:"` gives an in-memory database.
//! - [`open_read_only`] opens an existing file for reads only. It can read
//!   next to a live writer. It does not make the database file.
//! - [`execute`] runs one statement and returns the number of changed rows.
//! - [`query`] runs one statement and deserializes each row into `T`.
//!   [`query_one`] gives the first row. [`query_opt`] gives the first row or
//!   `None`.
//!
//! A connection is a [`Connection`]. Use its methods for transactions and
//! for a script of many statements (`db.execute_batch(sql)`).
//!
//! # Rows into types
//!
//! | `T` | Columns |
//! |-----|---------|
//! | struct, map, `serde_json::Value`, untagged enum | by name; other columns are ignored |
//! | tuple | by position; the counts must be equal |
//! | `Vec<T>` | one item for each column |
//! | scalar, `Option`, enum | the row must have one column |
//! | newtype | as its inner type |
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
//! Two columns with one name give an error for a struct or a map.
//!
//! A one-column BLOB row gives its bytes to a `Vec` or an array, for
//! example `Vec<u8>` or `[u8; 32]`. SQLite keeps a NaN float as `NULL`.
//!
//! In a `#[serde(flatten)]` part or an internally tagged enum, serde
//! buffers the fields. There, an `INTEGER` does not go into a `bool`. An
//! error does not name the column.
//!
//! # Errors
//!
//! An SQL error quotes the statement: `SQL error in "SELECT * FROM nope"`.
//! The cause is the [`rusqlite::Error`]. smop does not add parameter values
//! to an error. Thus give secrets as parameters, not in the SQL text. But
//! the SQLite message can show a value, for example a trigger message or an
//! `ATTACH` path. A row error gives the row number and the column, and can
//! show the value: `Failed to read row 2 of "SELECT ..."`, then
//! `column "age": invalid type: NULL, expected u32`.
//!
//! Open only database files that you trust. SQLite runs the views and
//! triggers of the file.
//!
//! # Build cost
//!
//! The `bundled` option compiles the SQLite C source with a C compiler.
//! Rust needs a C compiler to link, so this is not a new requirement. The
//! feature adds 9 crates, 14 with build dependencies. On a 4-core Linux
//! machine, a clean debug build takes 6 s more. A clean release build takes
//! about 50 s more. The C source is one file, so more cores do not make it
//! faster.
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

use std::borrow::Cow;
use std::path::Path;

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use rusqlite::types::Value;
use rusqlite::{ErrorCode, OpenFlags};
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
/// The path is a file path. A `file:` URI is not interpreted. This function
/// does not make parent directories. A statement waits up to 5 s for a lock
/// that another connection holds.
///
/// # Errors
///
/// Returns an error that names the path if SQLite cannot open the file.
/// Examples: the path is empty, the directory does not exist, or the file
/// is not a database.
///
/// # Examples
///
/// ```no_run
/// let db = smop::sqlite::open("target/app.db")?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn open<P: AsRef<Path>>(path: P) -> Result<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    open_with(path.as_ref(), flags, "")
}

/// Opens the existing database at `path` for reads only.
///
/// Use it to read a database that another process writes. Each statement
/// sees the last committed data. A write gives an error.
///
/// This function does not make the database file. For a database in WAL
/// mode, SQLite can make the `-wal` and `-shm` files. Thus the directory
/// must be writable.
///
/// # Errors
///
/// Returns an error that names the path if the path is empty or
/// `":memory:"`, if the file does not exist, or if it is not a database.
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
    if path.as_os_str() == ":memory:" {
        bail!("Failed to open SQLite database for reads: :memory: is not a file");
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    open_with(path, flags, " for reads")
}

/// The time that a statement waits for a lock.
const BUSY: Duration = Duration::from_secs(5);

fn open_with(path: &Path, flags: OpenFlags, mode: &str) -> Result<Connection> {
    let what = || format!("Failed to open SQLite database{mode}: {}", path.display());
    if path.as_os_str().is_empty() {
        // SQLite gives a temporary database. Its data would go at close.
        bail!("Failed to open SQLite database{mode}: the path is empty");
    }
    let db = Connection::open_with_flags(plain(path), flags).with_context(what)?;
    check(&db).with_context(what)?;
    Ok(db)
}

/// The bundled SQLite reads a name that starts with `file:` as a URI, with
/// any flags. `./` before it gives the same file without the URI.
fn plain(path: &Path) -> Cow<'_, Path> {
    if path.as_os_str().as_encoded_bytes().starts_with(b"file:") {
        Cow::Owned(Path::new(".").join(path))
    } else {
        Cow::Borrowed(path)
    }
}

/// SQLite reads the file at the first statement. Read the schema now, so
/// that a file that is not a database gives an error at open. Do not wait
/// for a lock: a locked file is a database.
fn check(db: &Connection) -> rusqlite::Result<()> {
    db.busy_timeout(Duration::ZERO)?;
    let read = db.query_row("SELECT count(*) FROM sqlite_schema", (), |_| Ok(()));
    db.busy_timeout(BUSY)?;
    match read {
        Err(e)
            if matches!(
                e.sqlite_error_code(),
                Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
            ) =>
        {
            Ok(())
        }
        other => other,
    }
}

/// An empty statement gives SQLite's "API misuse" error. Give a clear one.
fn not_empty(sql: &str) -> Result<()> {
    if sql.trim().is_empty() {
        bail!("SQL error in \"\": the statement is empty");
    }
    Ok(())
}

/// Runs one SQL statement. Returns the number of rows that it changed.
///
/// Give values as `params`, not in the SQL text: `()`, a tuple, an array,
/// [`params!`] or [`named_params!`]. For a statement that returns rows, use
/// [`query`].
///
/// # Errors
///
/// Returns an error that quotes the statement. Causes: the SQL is not
/// valid; the statement is empty; a constraint fails; the parameters do not
/// agree with the statement; `sql` has more than one statement; the
/// statement returns rows.
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
pub fn execute(db: &Connection, sql: &str, params: impl Params) -> Result<usize> {
    not_empty(sql)?;
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
pub fn query<T: DeserializeOwned>(
    db: &Connection,
    sql: &str,
    params: impl Params,
) -> Result<Vec<T>> {
    rows(db, sql, params, usize::MAX)
}

/// Runs one SQL statement and deserializes its first row into `T`.
///
/// This function does not read the other rows. It does not give an error
/// for them.
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
pub fn query_one<T: DeserializeOwned>(
    db: &Connection,
    sql: &str,
    params: impl Params,
) -> Result<T> {
    query_opt(db, sql, params)?.ok_or_else(|| anyhow!("No rows from {}", quoted(sql)))
}

/// Runs one SQL statement and deserializes its first row into `T`.
/// Returns `None` if there is no row.
///
/// Use it to find one row by its key. This function does not read the
/// other rows.
///
/// # Errors
///
/// As for [`query`].
///
/// # Examples
///
/// ```
/// use smop::sqlite;
///
/// let db = sqlite::open(":memory:")?;
/// let found: Option<(i64, String)> =
///     sqlite::query_opt(&db, "SELECT 1, 'a' WHERE ?1 = 1", [2])?;
/// assert_eq!(found, None);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn query_opt<T: DeserializeOwned>(
    db: &Connection,
    sql: &str,
    params: impl Params,
) -> Result<Option<T>> {
    Ok(rows(db, sql, params, 1)?.pop())
}

/// Reads at most `max` rows of `sql` into `T`.
fn rows<T: DeserializeOwned>(
    db: &Connection,
    sql: &str,
    params: impl Params,
    max: usize,
) -> Result<Vec<T>> {
    not_empty(sql)?;
    let fail = sql_error(sql);
    let mut stmt = db.prepare(sql).map_err(&fail)?;
    let names: Vec<String> = stmt.column_names().map(String::from).collect();
    let duplicate = de::duplicate(&names);
    let mut rows = stmt.query(params).map_err(&fail)?;
    let mut out = Vec::new();
    while out.len() < max {
        let Some(row) = rows.next().map_err(&fail)? else {
            break;
        };
        let n = out.len() + 1;
        let at = || format!("Failed to read row {n} of {}", quoted(sql));
        let mut values = Vec::with_capacity(names.len());
        for (i, name) in names.iter().enumerate() {
            let value = row
                .get::<_, Value>(i)
                .with_context(|| format!("column {name:?}"))
                .with_context(at)?;
            values.push(value);
        }
        let row = de::Row {
            names: &names,
            values,
            duplicate,
        };
        out.push(T::deserialize(row).with_context(at)?);
    }
    Ok(out)
}
