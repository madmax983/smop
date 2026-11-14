# ADR 0022: `sqlite` Feature

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

- The `sqlite` feature adds `smop::sqlite`. It is part of `full`. It
  implies `serde`.
- smop uses `rusqlite` 0.42 with the `bundled` option and the default
  features. A system SQLite is not necessary.
- smop re-exports `rusqlite` as `smop::rusqlite`. rusqlite types are in the
  API (`Connection`, `Params`), so `rusqlite` is on `ALLOWED` in
  `xtask/src/api_boundary.rs`.
- Version policy: a new rusqlite 0.x line in smop is a smop breaking change.
  A user who needs rusqlite uses `smop::rusqlite`, or the same 0.x line.
- The functions take a `&Connection`, so a `Transaction` also works:
  - `open(path)`: reads and writes; makes the file, not its parents.
    `":memory:"` is an in-memory database.
  - `open_read_only(path)`: no write and no new file.
  - `execute(db, sql, params) -> usize`: one statement; the changed rows.
  - `query::<T>(db, sql, params) -> Vec<T>` and `query_one::<T>`.
- Both `open` functions read the schema before they return. A file that is
  not a database gives an error that names the path at open, not at the
  first statement.
- smop has its own serde deserializer for a row. No `serde_rusqlite` crate.
  - Struct or map: by column name. Other columns are ignored. A missing
    `Option` field is `None`. Two columns with one name is an error.
  - Tuple: by position. The column count must be the tuple length.
  - Scalar, `Option`, enum or newtype: the row must have one column.
  - `INTEGER` gives integers (range checked by serde), floats and `bool`
    (0 is `false`, as in rusqlite). `TEXT` gives strings, `char` and unit
    variants. `BLOB` gives `Vec<u8>`. `NULL` gives `None`; for other types
    it is an error that says `NULL`.
  - Only `TEXT` gives a string. serde would take UTF-8 bytes from a BLOB;
    rusqlite does not, and smop does not.
- Errors:
  - An SQL error is `SQL error in "<statement>"`. The statement is trimmed
    and quoted with `{:?}`. The cause is the `rusqlite::Error`, so
    `downcast_ref` gives the SQLite error code.
  - The error does not show parameter values. They can be secrets.
  - A row error is `Failed to read row <n> of "<statement>"`, then
    `column "<name>": <serde error>`.
  - `query_one` with no row is `No rows from "<statement>"`.
- Not in this release: a wrapper for `execute_batch` (use the `Connection`
  method), serde parameters, and connection defaults such as WAL mode. A
  default that changes the file is a surprise for a script.

## Consequences

- Clean-build cost, measured on a 4-core Linux machine:
  - The feature adds 14 crates (`rusqlite`, `libsqlite3-sys`, `cc` and
    their dependencies) to `serde`.
  - A clean debug build of smop with `sqlite` takes 10 s; with `serde`
    only, 4 s.
  - A clean release build takes about 50 s. The C source is one file. One
    core compiles it, so more cores do not make it faster.
  - The build needs a C compiler. The Rust toolchain needs one for the link
    on all platforms, so this is not a new requirement.
- `full` includes `sqlite`. A crate on another rusqlite 0.x line must use
  `smop::rusqlite`, or turn off the default features of smop.
- rusqlite 0.42 needs Rust 1.89, smop's MSRV.
