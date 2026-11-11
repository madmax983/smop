- **`sh` spawn failures are a `sh::CommandError`** (#34), with the
  `io::Error` as its source. `err.downcast_ref::<io::Error>()` now gives
  `None`. The error text is the same. Migration: use
  `sh::exit_code(&err) == Some(127)`, or search `err.chain()` for the
  `io::Error`.
