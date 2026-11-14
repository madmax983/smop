# ADR 0023: `sqlite` Feature

## Status

Accepted (0.5 cycle). Issue [madmax983/smop#57](https://github.com/madmax983/smop/issues/57).

## Context

Python has `sqlite3`. Harvest's xtask reads SQLite files in its audit
tasks, and its Rhai example opens a read-only connection next to a live
writer. Both use `rusqlite` directly.

`libsqlite3-sys` declares `links = "sqlite3"`. Cargo accepts only one
crate with that key in a build, so a build has one rusqlite 0.x line. A
smop on rusqlite 0.42 and a crate on rusqlite 0.40 do not resolve together.

## Decision

- The `sqlite` feature adds `smop::sqlite`. It implies `serde`.
- `full` is every feature, so it includes `sqlite`. `default` is every
  feature but `sqlite`. Thus the default build has no C compile and no
  `links = "sqlite3"` key. Before this change, `default` was `full`.
- smop uses `rusqlite` 0.42 with the `bundled` option and the default
  features. smop does not need a system SQLite.
- smop re-exports `rusqlite` as `smop::rusqlite`. rusqlite types are in the
  API (`Connection`, `Params`), so `rusqlite` is on `ALLOWED` in
  `xtask/src/api_boundary.rs`.
- Version policy: a new rusqlite 0.x line in smop is a smop breaking change.
  A user who needs rusqlite uses `smop::rusqlite`, or the same 0.x line.
- The functions take a `&Connection`, so a `Transaction` and a `Savepoint`
  also work. Parameters are `impl Params`, so `query::<T>` needs no `_`.
  - `open(path)`: reads and writes; makes the file, not its parents.
    `":memory:"` is an in-memory database.
  - `open_read_only(path)`: no writes. It does not make the database file.
    For a database in WAL mode, SQLite can make the `-wal` and `-shm`
    files.
  - `execute(db, sql, params) -> usize`: one statement; the changed rows.
  - `query::<T>(db, sql, params) -> Vec<T>`.
  - `query_one::<T>`: the first row; no row is an error. Other rows are
    not read and give no error.
  - `query_opt::<T>`: the first row, or `None`.
- A path is a file path. The bundled SQLite reads a name that starts with
  `file:` as a URI, with or without `SQLITE_OPEN_URI`. Thus smop puts `./`
  before such a relative name. An empty path is an error: SQLite would give
  a temporary database, and its data would go at close.
  `open_read_only(":memory:")` is an error.
- Both `open` functions read the schema before they return. A file that is
  not a database gives an error that names the path at open, not at the
  first statement. This read does not wait for a lock: a locked file is a
  database. Later statements wait up to 5 s.
- smop has its own serde deserializer for a row. No `serde_rusqlite` crate.
  - Struct, map, or a type that reads any value (`serde_json::Value`, an
    untagged enum): by column name. Other columns are ignored. A missing
    `Option` field is `None`. Two columns with one name is an error, also
    for a map, which would keep only the last value.
  - Tuple: by position. The column count must be the tuple length.
  - Scalar, `Option` or enum: the row must have one column. A newtype is
    as its inner type.
  - `Vec<T>`: one item for each column. A one-column BLOB row gives its
    bytes to a `Vec` or an array.
  - `INTEGER` gives integers (range checked by serde), floats and `bool`
    (0 is `false`, as in rusqlite). `TEXT` gives strings, `char` and unit
    variants. `BLOB` is a sequence of bytes. `NULL` gives `None`; for other
    types it is an error that says `NULL`.
  - Only `TEXT` gives a string. serde would take UTF-8 bytes from a BLOB;
    rusqlite does not, and smop does not.
  - In a `#[serde(flatten)]` part or an internally tagged enum, serde
    buffers the fields. There, an `INTEGER` does not go into a `bool`, and
    an error does not name the column.
- Errors:
  - An SQL error is `SQL error in "<statement>"`. The statement is trimmed
    and quoted with `{:?}`. The cause is the `rusqlite::Error`, so
    `downcast_ref` gives the SQLite error code.
  - smop does not add parameter values to an error. The SQLite message can
    show one (a trigger message, an `ATTACH` path), and a row error can
    show a column value.
  - A row error is `Failed to read row <n> of "<statement>"`, then
    `column "<name>": <serde error>`.
  - `query_one` with no row is `No rows from "<statement>"`.
  - An empty statement is `SQL error in "": the statement is empty`.
- Not in this release: a wrapper for `execute_batch` (use the `Connection`
  method), serde parameters, and connection defaults such as WAL mode or
  `trusted_schema`. A default that changes the file is a surprise for a
  script.

## Consequences

- Clean-build cost, measured on a 4-core Linux machine:
  - The feature adds 9 crates to `serde`, 14 with build dependencies
    (`cc`, `pkg-config`, `vcpkg`, `find-msvc-tools`, `shlex`).
  - A clean debug build of smop with `sqlite` takes 10 s; with `serde`
    only, 4 s.
  - A clean release build takes about 50 s more. The C source is one file.
    One core compiles it, so more cores do not make it faster.
  - The build needs a C compiler. Rust needs one to link on all platforms,
    so this is not a new requirement.
- The default build does not change. But `full` is no longer on by
  default. cargo-semver-checks (`feature_not_enabled_by_default`) calls
  that a breaking change, so smop moves to 0.5.0.
- A crate that turns on `sqlite` or
  `full` and has its own rusqlite on another 0.x line must use
  `smop::rusqlite`.
- A new feature goes in both `default` and `full`, unless it has a cost
  like that of `sqlite`.
- rusqlite 0.42 needs Rust 1.89, smop's MSRV.
- Open only database files that you trust. SQLite runs the views and
  triggers of the file.
- In a read-only directory, a WAL database does not open for reads.
