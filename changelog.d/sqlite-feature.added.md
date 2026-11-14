- `sqlite` feature (in `full`): `sqlite::open`, `open_read_only`,
  `execute`, `query::<T>`, `query_one::<T>` and `query_opt::<T>`. Rows go
  into a struct by column name, into a tuple by position, or into a
  scalar. SQL errors quote the statement. smop does not add the parameter
  values. SQLite is bundled, so it needs no system library. A clean release
  build takes about 50 s more (ADR 0023).
- `smop::rusqlite` re-exports rusqlite 0.42. Cargo accepts one rusqlite 0.x
  line in a build. A crate with its own rusqlite on another line must use
  `smop::rusqlite`, or turn off the default features of smop.
